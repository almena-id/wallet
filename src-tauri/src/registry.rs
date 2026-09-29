//! Signing in to an Almena Registry portal, or linking this wallet to an
//! account there — the wallet's side of the registry's `…/auth/wallet` API.
//!
//! The portal shows `almena://auth?request_uri=…` as a link and a QR code. The
//! wallet reads the request at that address (who asks, for what, where the
//! answer goes), puts it to the person, and on Accept posts back an `id_token`
//! the way SIOPv2's `direct_post` does: a JWS, `EdDSA`, whose issuer and
//! subject are the `did:key` this wallet keeps for that portal, whose audience
//! is the portal and whose nonce is the request's. No DIDComm and no mediator:
//! plain HTTPS to the registry.
//!
//! **One key per registry**, derived like a pairwise: `m/5'/a'/b'/c'/n'`, with
//! `a`, `b`, `c` from a hash of the origin the request is read from (the
//! registry's API) and `n` the key's generation (0 until it is ever replaced).
//! The same words give the same DID for the same registry on any device, and
//! two registries cannot tell they are seeing the same person. Nothing is
//! stored. The key follows the origin the wallet actually reached, never the
//! `client_id` the request names: a page serving a request that names a real
//! portal gets a key of its own, which that portal does not know.
//!
//! The answer goes only to the host the request came from: a request that
//! sends it elsewhere is refused, so a page that copied a registry's request
//! cannot collect the token. And Accept answers the request the sheet showed,
//! kept from when it was read, not whatever the address serves a second time.
//!
//! **Signing.** A tenant admin signs a did:webvh log entry of one of the
//! tenant's identities the same way (`purpose: "sign"`): the request carries
//! the entry and the update keys that may sign it, and Accept answers with a
//! Data Integrity proof (`eddsa-jcs-2022`) by the same per-portal key, which
//! the registry checks before it publishes the entry. The wallet computes the
//! proof itself, over the JSON Canonicalization Scheme (RFC 8785) of the
//! entry, and refuses when its key is not one of the signers. What the sheet
//! shows about it (the DID, the version, the name, until when) must be what
//! the document says, or the request is refused.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::State;
use url::Url;
use zeroize::Zeroizing;

use crate::identity::keys::{self, REGISTRY};
use crate::identity::Held;
use crate::messaging::mediator::{client, transport};

/// The generation of the registry key in use: `n` in `m/5'/a'/b'/c'/n'`.
const GENERATION: u32 = 0;

/// How long a token is good for once signed.
const TOKEN_LIFETIME: u64 = 120;

#[derive(Debug, Clone, Copy)]
pub enum RegistryError {
    /// No identity is open, so there is no key to sign with.
    Locked,
    /// Not a registry request, or one that does not hold together.
    Unreadable,
    /// An address that would be reached over plain HTTP.
    Insecure,
    /// The registry did not answer, or its answer could not be read.
    Unreachable,
    /// The request has expired, or was answered already.
    Expired,
    /// The registry did not take the answer.
    Refused,
    /// Asked to sign with a key that is not one of the signers.
    NotASigner,
}

impl RegistryError {
    const fn code(self) -> &'static str {
        match self {
            Self::Locked => "registry_locked",
            Self::Unreadable => "registry_unreadable",
            Self::Insecure => "registry_insecure",
            Self::Unreachable => "registry_unreachable",
            Self::Expired => "registry_expired",
            Self::Refused => "registry_refused",
            Self::NotASigner => "registry_not_a_signer",
        }
    }
}

impl Serialize for RegistryError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// The request as the registry serves it.
#[derive(Deserialize, Clone)]
struct Request {
    response_type: String,
    response_mode: String,
    client_id: String,
    client_name: String,
    response_uri: String,
    nonce: String,
    purpose: String,
    #[serde(default)]
    sign: Option<Sign>,
}

/// `sign`: what to sign, and what it belongs to.
#[derive(Deserialize, Clone)]
struct Sign {
    /// `did_log_entry` or `endorsement`.
    kind: String,
    /// What it is about: an identity's or an item's name.
    identity: String,
    tenant: Option<String>,
    did: Option<String>,
    #[serde(default)]
    version: Option<u64>,
    #[serde(default)]
    valid_until: Option<String>,
    /// The keys that may sign it, as multikeys.
    signers: Vec<String>,
    /// The DID whose key signs (`{it}#{multikey}`); `None` for a log entry,
    /// signed as the key's own `did:key`.
    #[serde(default)]
    verification_method: Option<String>,
    proof_purpose: String,
    document: serde_json::Value,
    /// `endorsement`: the presentation to sign next, whose
    /// `verifiableCredential` takes the credential just signed.
    #[serde(default)]
    presentation: Option<serde_json::Value>,
}

const SIGN_KINDS: [&str; 2] = ["did_log_entry", "endorsement"];

/// What the confirmation sheet shows: who asks, for what, and as whom this
/// wallet would answer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRequest {
    /// The portal's name, as it gives it.
    name: String,
    /// The portal's host, which is what vouches for the name.
    portal: String,
    /// The host the answer goes to (the registry's API).
    answer_to: String,
    /// `sign_in`, `link` or `sign`.
    purpose: String,
    /// The DID this wallet is known by at that portal.
    did: String,
    /// `sign`: what is signed.
    signing: Option<Signing>,
}

/// What a signature request is for, as the sheet shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Signing {
    /// `did_log_entry`, or `endorsement` (a tenant vouches for one of its
    /// issuers, verifiers or mediators).
    kind: String,
    /// The identity's or the item's name; its tenant's.
    identity: String,
    tenant: Option<String>,
    /// The DID concerned; `None` for a first log entry, which makes it.
    did: Option<String>,
    version: Option<u64>,
    valid_until: Option<String>,
    /// This wallet's key is one of those that may sign it.
    signer: bool,
}

/// Is `input` a registry request (`almena://auth?request_uri=…`)? Read only.
pub fn is_request(input: &str) -> bool {
    request_uri(input).is_ok()
}

/// The request's address, from the link or the code.
fn request_uri(input: &str) -> Result<Url, RegistryError> {
    let link = Url::parse(input.trim()).map_err(|_| RegistryError::Unreadable)?;
    if link.scheme() != "almena" || link.host_str() != Some("auth") {
        return Err(RegistryError::Unreadable);
    }
    let uri = link
        .query_pairs()
        .find(|(key, _)| key == "request_uri")
        .map(|(_, value)| value.into_owned())
        .ok_or(RegistryError::Unreadable)?;
    Url::parse(&uri).map_err(|_| RegistryError::Unreadable)
}

/// The address to actually reach, under the same rules as a mediator's:
/// HTTPS, or plain HTTP for this machine in a debug build.
fn reachable(url: &Url) -> Result<String, RegistryError> {
    transport(url.as_str()).map_err(|_| RegistryError::Insecure)
}

/// Reads the request and checks that it holds together.
async fn fetch(input: &str) -> Result<(Url, Request), RegistryError> {
    let uri = request_uri(input)?;
    let response = client()
        .map_err(|_| RegistryError::Unreachable)?
        .get(reachable(&uri)?)
        .send()
        .await
        .map_err(|_| RegistryError::Unreachable)?;
    match response.status().as_u16() {
        200 => {}
        404 | 410 => return Err(RegistryError::Expired),
        _ => return Err(RegistryError::Unreachable),
    }
    let request: Request = response
        .json()
        .await
        .map_err(|_| RegistryError::Unreadable)?;
    let signing = request.purpose == "sign";
    if request.response_type != if signing { "proof" } else { "id_token" }
        || request.response_mode != "direct_post"
        || !matches!(request.purpose.as_str(), "sign_in" | "link" | "sign")
        || request.nonce.is_empty()
        || signing
            != request
                .sign
                .as_ref()
                .is_some_and(|s| SIGN_KINDS.contains(&s.kind.as_str()))
    {
        return Err(RegistryError::Unreadable);
    }
    let answer_to = Url::parse(&request.response_uri).map_err(|_| RegistryError::Unreadable)?;
    // The answer goes back where the request came from, and nowhere else.
    if answer_to.origin() != uri.origin() {
        return Err(RegistryError::Unreadable);
    }
    // The portal the sheet names must be vouched for by where the request
    // was read from, or any page could name any portal.
    if !vouched(&portal(&request.client_id)?, &uri) {
        return Err(RegistryError::Unreadable);
    }
    if request
        .sign
        .as_ref()
        .is_some_and(|sign| !told_as_it_is(sign))
    {
        return Err(RegistryError::Unreadable);
    }
    Ok((uri, request))
}

/// Is `portal` the registry's own, as seen from where its request was read?
/// The same host (`localhost` in development), or hosts one label under the
/// same parent — `registry.almena.id` and `api.almena.id`. A parent must have
/// a dot of its own, so `a.id` and `b.id` are strangers.
fn vouched(portal: &Url, read_from: &Url) -> bool {
    let (Some(portal), Some(read_from)) = (portal.host(), read_from.host()) else {
        return false;
    };
    if portal == read_from {
        return true;
    }
    let (url::Host::Domain(portal), url::Host::Domain(read_from)) = (portal, read_from) else {
        return false;
    };
    let parent = |host: &str| host.split_once('.').map(|(_, rest)| rest.to_owned());
    parent(portal)
        .filter(|rest| rest.contains('.'))
        .is_some_and(|rest| parent(read_from).as_ref() == Some(&rest))
}

/// Does what the sheet will show match the document that gets signed? The
/// DID, the version, the name and the validity are shown as the request
/// states them; here they are held to what the document itself says.
fn told_as_it_is(sign: &Sign) -> bool {
    let text = |value: &serde_json::Value| value.as_str().map(str::to_owned);
    let document = &sign.document;
    match sign.kind.as_str() {
        // A did:webvh log entry, signed as the key's own `did:key`.
        "did_log_entry" => {
            let version = document["versionId"]
                .as_str()
                .and_then(|id| id.split('-').next())
                .and_then(|number| number.parse::<u64>().ok());
            let made = text(&document["state"]["id"]);
            sign.verification_method.is_none()
                && sign.presentation.is_none()
                && sign.proof_purpose == "assertionMethod"
                && version.is_some()
                && version == sign.version
                && made.is_some()
                && (sign.did.is_none() || sign.did == made)
        }
        // The tenant's credential for one of its components, and that
        // component's presentation of it.
        "endorsement" => {
            let subject = &document["credentialSubject"];
            let holder = sign
                .presentation
                .as_ref()
                .and_then(|presentation| text(&presentation["holder"]));
            sign.did.is_some()
                && sign.proof_purpose == "assertionMethod"
                && text(&subject["id"]) == sign.did
                && holder == sign.did
                && text(&subject["name"]).as_deref() == Some(sign.identity.as_str())
                && text(&document["issuer"]) == sign.verification_method
                && sign.valid_until.is_some()
                && text(&document["validUntil"]) == sign.valid_until
        }
        _ => false,
    }
}

/// The portal a request names, as an origin: what the key is derived from.
fn portal(client_id: &str) -> Result<Url, RegistryError> {
    let url = Url::parse(client_id).map_err(|_| RegistryError::Unreadable)?;
    if !matches!(url.scheme(), "https" | "http") || url.host_str().is_none() {
        return Err(RegistryError::Unreadable);
    }
    Ok(url)
}

/// The key this wallet signs with at `origin`.
fn signing_key(seed: &[u8; 64], origin: &str, generation: u32) -> SigningKey {
    let hash = Sha256::digest(origin.as_bytes());
    // Three hardened steps of 31 bits each, as a pairwise takes them.
    let step = |at: usize| {
        u32::from_be_bytes([hash[at], hash[at + 1], hash[at + 2], hash[at + 3]]) & 0x7fff_ffff
    };
    let secret = keys::derive(seed, &[REGISTRY, step(0), step(4), step(8), generation]);
    SigningKey::from_bytes(&secret)
}

/// The `did:key` of a signing key.
fn did_of(key: &SigningKey) -> String {
    format!("did:key:{}", keys::written(&key.verifying_key().to_bytes()))
}

/// The key and DID for the registry at `uri`'s origin: where the request was
/// read from, not what it says about itself.
fn identity_for(seed: &[u8; 64], uri: &Url) -> (SigningKey, String) {
    let key = signing_key(seed, &uri.origin().ascii_serialization(), GENERATION);
    let did = did_of(&key);
    (key, did)
}

/// A compact JWS, `EdDSA`, over `claims`.
fn id_token(key: &SigningKey, did: &str, claims: serde_json::Value) -> String {
    let fragment = did.trim_start_matches("did:key:");
    let header = json!({"alg": "EdDSA", "typ": "JWT", "kid": format!("{did}#{fragment}")});
    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let signature = key.sign(signing_input.as_bytes());
    format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    )
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn seed(held: &Held) -> Result<Zeroizing<[u8; 64]>, RegistryError> {
    held.seed().ok_or(RegistryError::Locked)
}

/// The request the sheet is showing, with the link it came from: what Accept
/// answers.
#[derive(Default)]
pub struct Shown(Mutex<Option<(String, Url, Request)>>);

pub fn manage<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    app.manage(Shown::default());
}

/// Reads a registry request for the confirmation sheet, and keeps it for
/// Accept. Sends nothing.
#[tauri::command]
pub async fn registry_request(
    held: State<'_, Held>,
    shown: State<'_, Shown>,
    input: String,
) -> Result<RegistryRequest, RegistryError> {
    let seed = seed(&held)?;
    let (uri, request) = fetch(&input).await?;
    let (_, did) = identity_for(&seed, &uri);
    // Refused before anything is kept: the portal must at least be a web one.
    let portal = portal(&request.client_id)?;
    if let Ok(mut kept) = shown.0.lock() {
        *kept = Some((input, uri.clone(), request.clone()));
    }
    let signing = request.sign.as_ref().map(|sign| Signing {
        kind: sign.kind.clone(),
        identity: sign.identity.chars().take(200).collect(),
        tenant: sign.tenant.clone(),
        did: sign.did.clone(),
        version: sign.version,
        valid_until: sign.valid_until.clone(),
        signer: sign.signers.iter().any(|key| key == multikey(&did)),
    });
    Ok(RegistryRequest {
        signing,
        name: request.client_name.chars().take(80).collect(),
        portal: portal.host_str().unwrap_or_default().to_owned(),
        answer_to: uri.host_str().unwrap_or_default().to_owned(),
        purpose: request.purpose,
        did,
    })
}

/// Answers the registry request the sheet showed: what Accept does. The
/// request is not read again, so what is signed is what was shown; it is
/// kept until it is answered, so Accept can be tried again after a failure.
#[tauri::command]
pub async fn registry_answer(
    held: State<'_, Held>,
    shown: State<'_, Shown>,
    input: String,
) -> Result<(), RegistryError> {
    let seed = seed(&held)?;
    let kept = shown
        .0
        .lock()
        .ok()
        .and_then(|kept| kept.clone())
        .filter(|(link, _, _)| *link == input);
    let Some((_, uri, request)) = kept else {
        return Err(RegistryError::Unreadable);
    };
    answer(&seed, &uri, &request).await?;
    if let Ok(mut kept) = shown.0.lock() {
        if kept.as_ref().is_some_and(|(link, _, _)| *link == input) {
            *kept = None;
        }
    }
    Ok(())
}

async fn answer(seed: &[u8; 64], uri: &Url, request: &Request) -> Result<(), RegistryError> {
    let (key, did) = identity_for(seed, uri);
    if let Some(sign) = &request.sign {
        if !sign.signers.iter().any(|signer| signer == multikey(&did)) {
            return Err(RegistryError::NotASigner);
        }
        let named = sign.verification_method.as_deref().unwrap_or(&did);
        let created = timestamp(now());
        let proof = proof(&key, named, &sign.proof_purpose, &sign.document, &created)?;
        if sign.kind != "endorsement" {
            return post(&request.response_uri, Body::Json(json!({ "proof": proof }))).await;
        }
        // One approval, two signatures: the credential, then the presentation
        // that carries it — as the item's controller.
        let presentation = endorsed(&sign.document, &proof, sign.presentation.as_ref())?;
        let presented = self::proof(&key, named, "authentication", &presentation, &created)?;
        let body = json!({ "proof": proof, "presentation_proof": presented });
        return post(&request.response_uri, Body::Json(body)).await;
    }
    let issued = now();
    let token = id_token(
        &key,
        &did,
        json!({
            "iss": did,
            "sub": did,
            "aud": request.client_id,
            "nonce": request.nonce,
            "iat": issued,
            "exp": issued + TOKEN_LIFETIME,
        }),
    );
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("id_token", &token)
        .finish();
    post(&request.response_uri, Body::Form(body)).await
}

enum Body {
    Form(String),
    Json(serde_json::Value),
}

/// Posts the answer to where the request said, which `fetch` has checked is
/// where the request came from.
async fn post(response_uri: &str, body: Body) -> Result<(), RegistryError> {
    let answer_to = Url::parse(response_uri).map_err(|_| RegistryError::Unreadable)?;
    let (kind, text) = match body {
        Body::Form(text) => ("application/x-www-form-urlencoded", text),
        Body::Json(value) => ("application/json", value.to_string()),
    };
    let response = client()
        .map_err(|_| RegistryError::Unreachable)?
        .post(reachable(&answer_to)?)
        .header("Content-Type", kind)
        .body(text)
        .send()
        .await
        .map_err(|_| RegistryError::Unreachable)?;
    match response.status().as_u16() {
        200..=299 => Ok(()),
        404 | 409 | 410 => Err(RegistryError::Expired),
        _ => Err(RegistryError::Refused),
    }
}

/// The presentation of an endorsement: the template the registry sent, with
/// the credential just signed as its only `verifiableCredential`.
fn endorsed(
    credential: &serde_json::Value,
    proof: &serde_json::Value,
    template: Option<&serde_json::Value>,
) -> Result<serde_json::Value, RegistryError> {
    let mut presentation = template.cloned().ok_or(RegistryError::Unreadable)?;
    if !presentation.is_object() {
        return Err(RegistryError::Unreadable);
    }
    let mut signed = credential.clone();
    signed["proof"] = proof.clone();
    presentation["verifiableCredential"] = json!([signed]);
    Ok(presentation)
}

/// A `did:key`'s multikey: what update keys are written as.
fn multikey(did: &str) -> &str {
    did.trim_start_matches("did:key:")
}

/// An `eddsa-jcs-2022` Data Integrity proof over `document` by `key`:
/// SHA-256 of the canonical proof options, then of the canonical document,
/// signed; the signature in base58btc multibase.
fn proof(
    key: &SigningKey,
    named: &str,
    purpose: &str,
    document: &serde_json::Value,
    created: &str,
) -> Result<serde_json::Value, RegistryError> {
    if !matches!(purpose, "assertionMethod" | "authentication") {
        return Err(RegistryError::Unreadable);
    }
    // The key is named under `named` (its own `did:key`, or the DID it signs
    // for), with this wallet's multikey as the fragment.
    let own = keys::written(&key.verifying_key().to_bytes());
    let mut options = json!({
        "type": "DataIntegrityProof",
        "cryptosuite": "eddsa-jcs-2022",
        "verificationMethod": format!("{named}#{own}"),
        "created": created,
        "proofPurpose": purpose,
    });
    // A document with a context signs it as part of the proof options.
    if let Some(context) = document.get("@context") {
        options["@context"] = context.clone();
    }
    let mut hashed = Sha256::digest(jcs(&options)?.as_bytes()).to_vec();
    hashed.extend_from_slice(&Sha256::digest(jcs(document)?.as_bytes()));
    let signature = key.sign(&hashed);
    options["proofValue"] = json!(format!(
        "z{}",
        bs58::encode(signature.to_bytes()).into_string()
    ));
    Ok(options)
}

/// RFC 8785, the JSON Canonicalization Scheme, for what log entries hold:
/// keys sorted by UTF-16 code units, no whitespace, integers only.
fn jcs(value: &serde_json::Value) -> Result<String, RegistryError> {
    use serde_json::Value;
    Ok(match value {
        Value::Null | Value::Bool(_) | Value::String(_) => value.to_string(),
        Value::Number(number) if number.is_i64() || number.is_u64() => number.to_string(),
        Value::Number(_) => return Err(RegistryError::Unreadable),
        Value::Array(items) => {
            let parts: Result<Vec<String>, _> = items.iter().map(jcs).collect();
            format!("[{}]", parts?.join(","))
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let mut parts = Vec::with_capacity(keys.len());
            for key in keys {
                parts.push(format!(
                    "{}:{}",
                    Value::String(key.clone()),
                    jcs(&map[key])?
                ));
            }
            format!("{{{}}}", parts.join(","))
        }
    })
}

/// A moment as `YYYY-MM-DDTHH:MM:SSZ`, from seconds since the epoch.
fn timestamp(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    const SEED: [u8; 64] = [7; 64];

    /// Where a registry's request is read from.
    fn api() -> Url {
        Url::parse("https://api.almena.id/api/v1/auth/wallet/requests/a").unwrap()
    }

    fn log_entry() -> Sign {
        Sign {
            kind: "did_log_entry".into(),
            identity: "Acme".into(),
            tenant: Some("Acme".into()),
            did: Some("did:webvh:Qm:almena.id:ids:idn_a".into()),
            version: Some(2),
            valid_until: None,
            signers: vec![],
            verification_method: None,
            proof_purpose: "assertionMethod".into(),
            document: json!({
                "versionId": "2-Qm",
                "state": {"id": "did:webvh:Qm:almena.id:ids:idn_a"},
            }),
            presentation: None,
        }
    }

    fn endorsement() -> Sign {
        let item = "did:webvh:Qm:almena.id:ids:idn_i";
        let tenant = "did:webvh:Qm:almena.id:ids:idn_t";
        Sign {
            kind: "endorsement".into(),
            identity: "Issuer".into(),
            tenant: Some("Acme".into()),
            did: Some(item.into()),
            version: None,
            valid_until: Some("2027-09-30T10:00:00Z".into()),
            signers: vec![],
            verification_method: Some(tenant.into()),
            proof_purpose: "assertionMethod".into(),
            document: json!({
                "issuer": tenant,
                "validUntil": "2027-09-30T10:00:00Z",
                "credentialSubject": {"id": item, "name": "Issuer", "memberOf": {"id": tenant}},
            }),
            presentation: Some(json!({"holder": item, "verifiableCredential": []})),
        }
    }

    #[test]
    fn what_is_shown_must_be_what_is_signed() {
        assert!(told_as_it_is(&log_entry()));
        // A first entry names no DID yet; the one it makes is in the entry.
        assert!(told_as_it_is(&Sign {
            did: None,
            ..log_entry()
        }));
        assert!(!told_as_it_is(&Sign {
            version: Some(3),
            ..log_entry()
        }));
        assert!(!told_as_it_is(&Sign {
            did: Some("did:webvh:Qm:almena.id:ids:idn_other".into()),
            ..log_entry()
        }));
        assert!(!told_as_it_is(&Sign {
            verification_method: Some("did:webvh:Qm:almena.id".into()),
            ..log_entry()
        }));

        assert!(told_as_it_is(&endorsement()));
        assert!(!told_as_it_is(&Sign {
            identity: "Other".into(),
            ..endorsement()
        }));
        assert!(!told_as_it_is(&Sign {
            valid_until: Some("2099-01-01T00:00:00Z".into()),
            ..endorsement()
        }));
        assert!(!told_as_it_is(&Sign {
            verification_method: Some("did:webvh:Qm:almena.id:ids:idn_x".into()),
            ..endorsement()
        }));
        assert!(!told_as_it_is(&Sign {
            presentation: Some(json!({"holder": "did:webvh:Qm:almena.id:ids:idn_x"})),
            ..endorsement()
        }));
        assert!(!told_as_it_is(&Sign {
            kind: "anything".into(),
            ..endorsement()
        }));
    }

    #[test]
    fn a_link_is_read_only_as_a_registry_request() {
        let link = "almena://auth?request_uri=https%3A%2F%2Fapi.almena.id%2Fapi%2Fv1%2Fauth%2Fwallet%2Frequests%2Fabc";
        assert_eq!(
            request_uri(link).unwrap().as_str(),
            "https://api.almena.id/api/v1/auth/wallet/requests/abc"
        );
        assert!(!is_request("almena://invite?_oob=abc"));
        assert!(!is_request("https://api.almena.id/x"));
        assert!(!is_request("almena://auth"));
    }

    #[test]
    fn each_registry_sees_its_own_did_and_always_the_same() {
        let at = |uri: &str| identity_for(&SEED, &Url::parse(uri).unwrap()).1;
        let one = at("https://api.almena.id/api/v1/auth/wallet/requests/a");
        let again = at("https://api.almena.id/api/v1/auth/wallet/requests/b");
        let other = at("https://registry.example.org/api/v1/auth/wallet/requests/a");
        assert_eq!(one, again);
        assert_ne!(one, other);
        assert!(one.starts_with("did:key:z6Mk"));

        let root = did_of(&keys::signing_key(&SEED));
        assert_ne!(one, root, "never the identity's own key");

        let next = did_of(&signing_key(&SEED, "https://api.almena.id", 1));
        assert_ne!(one, next, "a new generation is a new key");
    }

    #[test]
    fn the_token_verifies_with_the_did_it_names() {
        let (key, did) = identity_for(&SEED, &api());
        let token = id_token(&key, &did, json!({"sub": did, "nonce": "n"}));
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);

        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["alg"], "EdDSA");
        assert!(header["kid"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{did}#")));

        // The key comes back out of the DID: multibase, then the multicodec prefix.
        let multibase = did.trim_start_matches("did:key:z");
        let raw = bs58::decode(multibase).into_vec().unwrap();
        assert_eq!(&raw[..2], &[0xed, 0x01]);
        let public = VerifyingKey::from_bytes(raw[2..].try_into().unwrap()).unwrap();
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
        public
            .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
            .unwrap();
    }

    /// Against a registry API run on this machine, whose requests name it:
    /// `REGISTRY_PUBLIC_URL=http://localhost:8000 task dev` in `../api` (or set
    /// `ALMENA_TEST_REGISTRY`), then `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "needs a registry API on this machine"]
    async fn a_local_registry_signs_the_wallet_in() {
        let api = std::env::var("ALMENA_TEST_REGISTRY")
            .unwrap_or_else(|_| "http://localhost:8000".to_owned());
        let http = client().unwrap();
        let asked: serde_json::Value = http
            .post(format!("{api}/api/v1/auth/wallet/requests"))
            .json(&json!({"purpose": "sign_in"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let (uri, request) = fetch(asked["deep_link"].as_str().unwrap()).await.unwrap();
        answer(&SEED, &uri, &request).await.unwrap();

        let result: serde_json::Value = http
            .post(format!(
                "{api}/api/v1/auth/wallet/requests/{}/result",
                asked["id"].as_str().unwrap()
            ))
            .json(&json!({"poll": asked["poll"]}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(result["status"], "signed_in", "{result}");

        // A request answered once is not answered again.
        let again = answer(&SEED, &uri, &request).await;
        assert!(matches!(again, Err(RegistryError::Expired)));

        // The account the wallet signed up is its tenant's admin, and the
        // wallet one of the update keys: it signs the tenant's identity.
        let token = result["session"]["token"].as_str().unwrap();
        let get = |path: String| {
            http.get(format!("{api}/api/v1{path}"))
                .bearer_auth(token)
                .send()
        };
        let tenants: serde_json::Value =
            get("/tenants".into()).await.unwrap().json().await.unwrap();
        let tenant = tenants[0]["id"].as_str().unwrap();
        let detail: serde_json::Value = get(format!("/tenants/{tenant}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let identity = detail["identity"]["id"].as_str().unwrap();
        let asked: serde_json::Value = http
            .post(format!(
                "{api}/api/v1/tenants/{tenant}/identities/{identity}/sign"
            ))
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let (uri, request) = fetch(asked["deep_link"].as_str().unwrap()).await.unwrap();
        answer(&SEED, &uri, &request).await.unwrap();
        let result: serde_json::Value = http
            .post(format!(
                "{api}/api/v1/auth/wallet/requests/{}/result",
                asked["id"].as_str().unwrap()
            ))
            .bearer_auth(token)
            .json(&json!({"poll": asked["poll"]}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(result["status"], "signed", "{result}");
        let signed: serde_json::Value = get(format!("/tenants/{tenant}/identities/{identity}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            signed["did"].as_str().unwrap().starts_with("did:webvh:"),
            "{signed}"
        );
    }

    #[test]
    fn jcs_sorts_by_utf16_and_writes_no_space() {
        let value = json!({"b": [1, true, null], "a": "é", "\u{10000}": 1, "\u{ff61}": 2});
        // U+10000 is a surrogate pair (D800…), before U+FF61 in UTF-16.
        assert_eq!(
            jcs(&value).unwrap(),
            "{\"a\":\"é\",\"b\":[1,true,null],\"\u{10000}\":1,\"\u{ff61}\":2}"
        );
        assert!(jcs(&json!({"x": 1.5})).is_err());
    }

    #[test]
    fn the_proof_verifies_over_the_canonical_entry() {
        let (key, did) = identity_for(&SEED, &api());
        let entry = json!({"versionId": "1-Qm", "state": {"id": "did:webvh:Qm:almena.id"}});
        let proof = proof(
            &key,
            &did,
            "assertionMethod",
            &entry,
            "2026-09-30T10:00:00Z",
        )
        .unwrap();
        assert_eq!(
            proof["verificationMethod"],
            format!("{did}#{}", multikey(&did))
        );

        let mut options = proof.clone();
        options.as_object_mut().unwrap().remove("proofValue");
        let mut hashed = Sha256::digest(jcs(&options).unwrap().as_bytes()).to_vec();
        hashed.extend_from_slice(&Sha256::digest(jcs(&entry).unwrap().as_bytes()));
        let value = proof["proofValue"].as_str().unwrap();
        let bytes = bs58::decode(&value[1..]).into_vec().unwrap();
        let signature = Signature::from_slice(&bytes).unwrap();
        key.verifying_key().verify(&hashed, &signature).unwrap();
    }

    #[test]
    fn a_document_is_signed_as_a_key_of_the_did_it_signs_for() {
        let (key, did) = identity_for(&SEED, &api());
        let credential = json!({
            "@context": ["https://www.w3.org/ns/credentials/v2"],
            "type": ["VerifiableCredential"],
        });
        let issuer = "did:webvh:Qm:almena.id";
        let signed = proof(
            &key,
            issuer,
            "authentication",
            &credential,
            "2026-09-30T10:00:00Z",
        )
        .unwrap();
        assert_eq!(
            signed["verificationMethod"],
            format!("{issuer}#{}", multikey(&did))
        );
        assert_eq!(signed["proofPurpose"], "authentication");
        // The document's context travels in the proof options.
        assert_eq!(signed["@context"], credential["@context"]);
        assert!(proof(&key, issuer, "keyAgreement", &credential, "x").is_err());
    }

    #[test]
    fn an_endorsement_presents_the_credential_just_signed() {
        let (key, _) = identity_for(&SEED, &api());
        let tenant = "did:webvh:Qm:almena.id:ids:idn_t";
        let credential = json!({"@context": ["https://www.w3.org/ns/credentials/v2"], "id": "x"});
        let template = json!({
            "@context": ["https://www.w3.org/ns/credentials/v2"],
            "type": ["VerifiablePresentation"],
            "holder": "did:webvh:Qm:almena.id:ids:idn_i",
            "verifiableCredential": [],
        });
        let signed = proof(&key, tenant, "assertionMethod", &credential, "t").unwrap();
        let presentation = endorsed(&credential, &signed, Some(&template)).unwrap();
        assert_eq!(presentation["verifiableCredential"][0]["proof"], signed);
        assert_eq!(presentation["verifiableCredential"][0]["id"], "x");
        assert_eq!(presentation["holder"], template["holder"]);
        assert!(endorsed(&credential, &signed, None).is_err());
    }

    #[test]
    fn timestamps_are_utc_seconds() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(1_790_683_200), "2026-09-29T12:00:00Z");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn a_portal_is_named_only_by_its_own_registry() {
        let at = |portal: &str, api: &str| {
            vouched(&Url::parse(portal).unwrap(), &Url::parse(api).unwrap())
        };
        assert!(at("https://registry.almena.id", "https://api.almena.id/x"));
        assert!(at("http://localhost:3000", "http://localhost:8000/x"));
        assert!(at("https://almena.id", "https://almena.id/x"));
        assert!(!at("https://registry.almena.id", "https://evil.example/x"));
        assert!(!at(
            "https://registry.almena.id",
            "https://api.almena.id.evil.example/x"
        ));
        assert!(!at("https://a.id", "https://b.id/x"));
        assert!(!at("http://127.0.0.1:3000", "http://127.0.0.2:8000/x"));
    }

    #[test]
    fn only_web_portals_are_named() {
        assert!(portal("https://registry.almena.id").is_ok());
        assert!(portal("almena://auth").is_err());
        assert!(portal("not a url").is_err());
    }
}
