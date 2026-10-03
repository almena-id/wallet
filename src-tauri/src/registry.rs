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
//!
//! **Applying for a credential.** A holder applies to one of the registry's
//! issuers from the portal, in three kinds of request that name the issuer:
//! `pair` (an `id_token`, as for signing in), `present` (an OpenID4VP request
//! with a DCQL query, answered with a `vp_token`) and `submit` (a JWS over the
//! application). Each issuer gets a key of its own, derived like the
//! registry's but from the registry's origin *and* the issuer's DID, so two
//! issuers cannot tell they see the same person; pairing and submitting use
//! the same one. `submit` carries the application's content — every answer,
//! readable — and its digest (SHA-256 of its JCS): the wallet recomputes the
//! digest before showing anything, shows that content, and signs the digest
//! with the application's id, so what is signed is what was shown. `present`
//! answers the request's DCQL query with the credentials this wallet holds
//! (`presentation.rs`): the sheet shows each one and only the claims that would
//! go, and Accept sends that same choice as a `vp_token`.
//!
//! **Issuing.** The issuer's signer signs the credential an application ends
//! in the same way (`purpose: "sign"`, kind `credential`): the request carries
//! an SD-JWT VC's header, payload and disclosures; the sheet shows the claims
//! (each disclosure checked against the payload's `_sd`, all of them shown),
//! the issuer and until when, and Accept answers with a JWS over that header
//! and payload by this wallet's registry key — the one the issuer's DID lists.
//! The holder then takes it (`purpose: "receive"`): an `id_token` by the key
//! paired with that issuer, answered with the credential, which is checked —
//! bound to that key, from that issuer, every disclosure listed — and kept
//! sealed (`credentials.rs`).
//!
//! **Status lists.** The issuer's signer also signs its status list (kind
//! `status_list`, a `statuslist+jwt`): before its first credential, and each
//! time one is suspended, reinstated or revoked. The sheet shows the status a
//! credential is given only after reading it from the list that is signed.

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
    /// Asked to submit an application paired with another key.
    NotTheHolder,
    /// Asked to present credentials this wallet does not hold.
    NothingToPresent,
    /// Given a credential its issuer's DID does not vouch for: not signed by
    /// a key the issuer's document lists.
    IssuerUnverified,
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
            Self::NotTheHolder => "registry_not_the_holder",
            Self::NothingToPresent => "registry_nothing_to_present",
            Self::IssuerUnverified => "registry_issuer_unverified",
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
    /// `pair`, `present`, `submit`: the issuer applied to.
    #[serde(default)]
    issuer: Option<IssuerRef>,
    /// The credential applied for: its id and names.
    #[serde(default)]
    credential_type: Option<CredentialTypeRef>,
    /// The DID the application was paired with, once it was.
    #[serde(default)]
    holder: Option<String>,
    /// `present`: the OpenID4VP DCQL query.
    #[serde(default)]
    dcql_query: Option<serde_json::Value>,
    /// `submit`: what is signed.
    #[serde(default)]
    submission: Option<Submission>,
    /// `verify`: the verifier asking, by DID and name.
    #[serde(default)]
    verifier: Option<IssuerRef>,
    /// `verify`: what it asks for, its name by language.
    #[serde(default)]
    form: Option<FormRef>,
}

/// `verify`: the form a verifier asks to be answered, by its name.
#[derive(Deserialize, Clone)]
struct FormRef {
    #[serde(default)]
    name: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize, Clone)]
struct IssuerRef {
    did: String,
    name: String,
}

#[derive(Deserialize, Clone)]
struct CredentialTypeRef {
    id: String,
    #[serde(default)]
    labels: std::collections::BTreeMap<String, String>,
}

/// `submit`: the application's content and its digest.
#[derive(Deserialize, Clone)]
struct Submission {
    content: serde_json::Value,
    digest: String,
}

const APPLYING: [&str; 4] = ["pair", "present", "submit", "receive"];

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
    /// `status_list`: the entry whose status the list changes, and to what;
    /// `None` when it is signed as it is.
    #[serde(default)]
    status_change: Option<StatusChange>,
}

/// An entry of an issuer's status list, and the status it is given.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct StatusChange {
    index: u64,
    /// `valid`, `suspended` or `revoked`.
    status: String,
    /// The credential's holder, as the issuer knows them.
    #[serde(default)]
    holder: Option<String>,
}

const SIGN_KINDS: [&str; 4] = ["did_log_entry", "endorsement", "credential", "status_list"];

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
    /// `pair`, `present`, `submit`: the application.
    applying: Option<Applying>,
    /// `verify`: the verifier, and what would be presented to it.
    verifying: Option<Verifying>,
}

/// A verifier asking for credentials, as the sheet shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verifying {
    verifier: String,
    verifier_did: String,
    /// What it asks for: its form's name, by language.
    form: std::collections::BTreeMap<String, String>,
    /// How many credentials are asked for.
    asked: usize,
    /// What this wallet would present, credential by credential.
    presenting: Vec<Presenting>,
    /// Every credential it requires is among them.
    complete: bool,
}

/// An application, as the sheet shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applying {
    issuer: String,
    issuer_did: String,
    /// The credential applied for, by language.
    credential: std::collections::BTreeMap<String, String>,
    /// `submit`: every answer, by language, and whether a credential vouched for it.
    answers: Vec<Answer>,
    /// `present`: how many credentials are asked for.
    asked: usize,
    /// `present`: what this wallet would present, credential by credential.
    presenting: Vec<Presenting>,
    /// `present`: every credential the form requires is among them.
    complete: bool,
}

/// One credential to present, as the sheet shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presenting {
    /// Its type's names, by language.
    credential: std::collections::BTreeMap<String, String>,
    issuer: String,
    /// The claims that would be disclosed; nothing else of it goes.
    claims: Vec<Claim>,
}

/// What this wallet would present for a request, from the credentials it holds.
fn presentable<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    request: &Request,
) -> (Vec<crate::presentation::Chosen>, bool) {
    let held = crate::credentials::all(app, seed).unwrap_or_default();
    request
        .dcql_query
        .as_ref()
        .map(|query| crate::presentation::choose(query, &held, now()))
        .unwrap_or((Vec::new(), false))
}

fn claim(name: &str, value: &serde_json::Value) -> Claim {
    Claim {
        name: name.to_owned(),
        value: value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned)
            .chars()
            .take(500)
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    label: std::collections::BTreeMap<String, String>,
    text: std::collections::BTreeMap<String, String>,
    verified: bool,
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
    /// `credential`: what it says, claim by claim.
    claims: Vec<Claim>,
    /// `credential`: its type (`vct`) and the holder it is bound to
    /// (`cnf.kid`).
    credential_type: Option<String>,
    holder: Option<String>,
    /// `status_list`: the status it changes, as the list says it.
    change: Option<StatusChange>,
}

/// One claim of a credential to sign, as the sheet shows it.
#[derive(Serialize)]
pub struct Claim {
    name: String,
    /// A string as it is; anything else as JSON.
    value: String,
}

/// A credential's disclosures, decoded: `[salt, name, value]` each, every one
/// listed in the payload's `_sd` and every listed one there.
fn disclosed(document: &serde_json::Value) -> Option<Vec<(String, serde_json::Value)>> {
    let listed: Vec<&str> = document["payload"]["_sd"]
        .as_array()?
        .iter()
        .filter_map(|item| item.as_str())
        .collect();
    let disclosures = document["disclosures"].as_array()?;
    if disclosures.len() != listed.len() {
        return None;
    }
    let mut claims: Vec<(String, serde_json::Value)> = Vec::with_capacity(disclosures.len());
    for disclosure in disclosures {
        let text = disclosure.as_str()?;
        let digest = URL_SAFE_NO_PAD.encode(Sha256::digest(text.as_bytes()));
        if !listed.contains(&digest.as_str()) {
            return None;
        }
        let decoded: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(text).ok()?).ok()?;
        let name = decoded[1].as_str()?.to_owned();
        if decoded.as_array()?.len() != 3 || claims.iter().any(|(seen, _)| *seen == name) {
            return None;
        }
        claims.push((name, decoded[2].clone()));
    }
    Some(claims)
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
    let expected = match request.purpose.as_str() {
        "sign" => "proof",
        "present" | "verify" => "vp_token",
        "submit" => "signature",
        "receive" => "credential",
        _ => "id_token",
    };
    if request.response_type != expected
        || request.response_mode != "direct_post"
        || !matches!(
            request.purpose.as_str(),
            "sign_in" | "link" | "sign" | "pair" | "present" | "submit" | "receive" | "verify"
        )
        || request.nonce.is_empty()
        || !applies_as_it_says(&request)
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

/// Does an application's request hold together? A verifier's (`verify`)
/// names the verifier and its query. An application's names its issuer;
/// `present` brings its query; `submit` its content, whose digest is the
/// one the wallet would sign and whose issuer is the one named. Any other
/// request names none of it.
fn applies_as_it_says(request: &Request) -> bool {
    // A verifier asking: it names itself and its query, and nothing of an
    // application.
    if request.purpose == "verify" {
        return request
            .verifier
            .as_ref()
            .is_some_and(|verifier| !verifier.did.is_empty())
            && request.dcql_query.is_some()
            && request.issuer.is_none()
            && request.submission.is_none();
    }
    if request.verifier.is_some() {
        return false;
    }
    let applying = APPLYING.contains(&request.purpose.as_str());
    if !applying {
        return request.issuer.is_none()
            && request.submission.is_none()
            && request.dcql_query.is_none();
    }
    let Some(issuer) = &request.issuer else {
        return false;
    };
    if issuer.did.is_empty() || request.credential_type.is_none() {
        return false;
    }
    match request.purpose.as_str() {
        "present" => request.dcql_query.is_some(),
        "submit" => request.submission.as_ref().is_some_and(|submission| {
            let content = &submission.content;
            content["application"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
                && content["issuer"].as_str() == Some(issuer.did.as_str())
                && digest_of(content).as_deref() == Some(submission.digest.as_str())
        }),
        "receive" => request.response_type == "credential",
        _ => true,
    }
}

/// The digest an application is signed by: SHA-256 of its JCS, in hex.
fn digest_of(content: &serde_json::Value) -> Option<String> {
    let canonical = jcs(content).ok()?;
    Some(
        Sha256::digest(canonical.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// Is `portal` the registry's own, as seen from where its request was read?
/// The same host (`localhost` in development), or hosts one label under the
/// same parent — `registry.almena.id` and `api.almena.id`. The parent must be
/// somebody's domain and not a public suffix (the Public Suffix List, private
/// section included), so `a.id` and `b.id` are strangers, and so are
/// `evil.github.io` and `victim.github.io` or `a.co.uk` and `b.co.uk`.
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
        .filter(|rest| psl::domain(rest.as_bytes()).is_some())
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
        // An issuer's credential: signed as a key of its DID, every claim
        // shown, valid until when the sheet says.
        "credential" => {
            let header = &document["header"];
            let payload = &document["payload"];
            let issuer = sign.verification_method.clone().unwrap_or_default();
            let kid = text(&header["kid"]).unwrap_or_default();
            sign.presentation.is_none()
                && sign.proof_purpose == "assertionMethod"
                && !issuer.is_empty()
                && sign.did.as_deref() == Some(issuer.as_str())
                && text(&payload["iss"]).as_deref() == Some(issuer.as_str())
                && text(&header["alg"]).as_deref() == Some("EdDSA")
                && text(&header["typ"]).as_deref() == Some("dc+sd-jwt")
                && sign
                    .signers
                    .iter()
                    .any(|key| kid == format!("{issuer}#{key}"))
                && payload["exp"]
                    .as_u64()
                    .is_some_and(|exp| sign.valid_until.as_deref() == Some(timestamp(exp).as_str()))
                && disclosed(document).is_some()
                && nothing_unshown(document)
        }
        // An issuer's status list: signed as a key of its DID, saying what
        // the sheet says it changes.
        "status_list" => {
            let header = &document["header"];
            let issuer = sign.verification_method.clone().unwrap_or_default();
            let kid = text(&header["kid"]).unwrap_or_default();
            let changed = match &sign.status_change {
                Some(change) => {
                    let value = match change.status.as_str() {
                        "valid" => Some(0),
                        "revoked" => Some(1),
                        "suspended" => Some(2),
                        _ => None,
                    };
                    value.is_some() && status_at(document, change.index) == value
                }
                None => status_at(document, 0).is_some(),
            };
            sign.presentation.is_none()
                && sign.proof_purpose == "assertionMethod"
                && !issuer.is_empty()
                && sign.did.as_deref() == Some(issuer.as_str())
                && text(&header["alg"]).as_deref() == Some("EdDSA")
                && text(&header["typ"]).as_deref() == Some("statuslist+jwt")
                && sign
                    .signers
                    .iter()
                    .any(|key| kid == format!("{issuer}#{key}"))
                && text(&document["payload"]["sub"])
                    .and_then(|sub| Url::parse(&sub).ok())
                    .is_some_and(|sub| reachable(&sub).is_ok())
                && changed
        }
        _ => false,
    }
}

/// Does a credential to sign say nothing beyond what the sheet shows or the
/// wallet checks? Every claim travels as a disclosure, listed in `_sd` and
/// shown one by one; what else the payload may carry is fixed: the issuer, the
/// times, the type (`vct`) and the holder (`cnf.kid`), both shown, the digest
/// algorithm, and the entry in the issuer's status list. A claim written in
/// the clear, a key bound some other way or one header field more is refused:
/// it would be signed without anybody having seen it.
fn nothing_unshown(document: &serde_json::Value) -> bool {
    let only = |value: &serde_json::Value, allowed: &[&str]| {
        value
            .as_object()
            .is_some_and(|map| map.keys().all(|key| allowed.contains(&key.as_str())))
    };
    let header = &document["header"];
    let payload = &document["payload"];
    let status = &payload["status"];
    only(header, &["alg", "typ", "kid"])
        && only(
            payload,
            &[
                "iss", "iat", "exp", "vct", "cnf", "_sd_alg", "_sd", "status",
            ],
        )
        && payload["_sd_alg"].as_str() == Some("sha-256")
        && payload["iat"]
            .as_u64()
            .is_some_and(|iat| payload["exp"].as_u64().is_some_and(|exp| iat < exp))
        && payload["vct"].as_str().is_some_and(|vct| !vct.is_empty())
        && only(&payload["cnf"], &["kid"])
        && payload["cnf"]["kid"]
            .as_str()
            .is_some_and(|kid| kid.starts_with("did:"))
        && (status.is_null()
            || (only(status, &["status_list"])
                && only(&status["status_list"], &["idx", "uri"])
                && status["status_list"]["idx"].as_u64().is_some()
                && status["status_list"]["uri"]
                    .as_str()
                    .and_then(|uri| Url::parse(uri).ok())
                    .is_some_and(|uri| reachable(&uri).is_ok())))
}

/// The status at `index` of a Status List Token document's payload.
fn status_at(document: &serde_json::Value, index: u64) -> Option<u8> {
    crate::status::status_at(&document["payload"], index)
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
/// read from, not what it says about itself. Applying to one of its issuers,
/// one of the issuer's own: the origin and the issuer's DID together.
fn identity_for(seed: &[u8; 64], uri: &Url, issuer: Option<&str>) -> (SigningKey, String) {
    let origin = uri.origin().ascii_serialization();
    let scope = match issuer {
        Some(did) => format!("{origin} {did}"),
        None => origin,
    };
    let key = signing_key(seed, &scope, GENERATION);
    let did = did_of(&key);
    (key, did)
}

/// The issuer a request applies to, if it applies to one.
fn issuer_of(request: &Request) -> Option<&str> {
    APPLYING
        .contains(&request.purpose.as_str())
        .then(|| request.issuer.as_ref().map(|issuer| issuer.did.as_str()))
        .flatten()
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
    app: tauri::AppHandle,
    held: State<'_, Held>,
    shown: State<'_, Shown>,
    input: String,
) -> Result<RegistryRequest, RegistryError> {
    let seed = seed(&held)?;
    let (uri, request) = fetch(&input).await?;
    let (_, did) = identity_for(&seed, &uri, issuer_of(&request));
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
        claims: if sign.kind == "credential" {
            disclosed(&sign.document)
                .unwrap_or_default()
                .into_iter()
                .map(|(name, value)| Claim {
                    name,
                    value: value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                        .chars()
                        .take(500)
                        .collect(),
                })
                .collect()
        } else {
            Vec::new()
        },
        credential_type: (sign.kind == "credential")
            .then(|| sign.document["payload"]["vct"].as_str().map(str::to_owned))
            .flatten(),
        holder: (sign.kind == "credential")
            .then(|| {
                sign.document["payload"]["cnf"]["kid"]
                    .as_str()
                    .map(str::to_owned)
            })
            .flatten(),
        change: sign.status_change.clone(),
    });
    let (chosen, complete) = if request.purpose == "present" || request.purpose == "verify" {
        presentable(&app, &seed, &request)
    } else {
        (Vec::new(), true)
    };
    let presenting = || -> Vec<Presenting> {
        chosen
            .iter()
            .map(|one| Presenting {
                credential: one.credential.type_labels.clone(),
                issuer: one.credential.issuer_name.clone(),
                claims: one
                    .claims
                    .iter()
                    .map(|(name, value)| claim(name, value))
                    .collect(),
            })
            .collect()
    };
    let asked = request
        .dcql_query
        .as_ref()
        .and_then(|query| query["credential_sets"].as_array().map(Vec::len))
        .unwrap_or(0);
    let verifying = request.verifier.as_ref().map(|verifier| Verifying {
        verifier: verifier.name.chars().take(200).collect(),
        verifier_did: verifier.did.clone(),
        form: request
            .form
            .as_ref()
            .map(|form| form.name.clone())
            .unwrap_or_default(),
        asked,
        presenting: presenting(),
        complete,
    });
    let applying = request.issuer.as_ref().map(|issuer| Applying {
        presenting: chosen
            .iter()
            .map(|one| Presenting {
                credential: one.credential.type_labels.clone(),
                issuer: one.credential.issuer_name.clone(),
                claims: one
                    .claims
                    .iter()
                    .map(|(name, value)| claim(name, value))
                    .collect(),
            })
            .collect(),
        complete,
        issuer: issuer.name.chars().take(200).collect(),
        issuer_did: issuer.did.clone(),
        credential: request
            .credential_type
            .as_ref()
            .map(|kind| {
                if kind.labels.is_empty() {
                    std::collections::BTreeMap::from([("en".to_owned(), kind.id.clone())])
                } else {
                    kind.labels.clone()
                }
            })
            .unwrap_or_default(),
        answers: request
            .submission
            .as_ref()
            .map(|submission| answers(&submission.content))
            .unwrap_or_default(),
        asked: request
            .dcql_query
            .as_ref()
            .and_then(|query| query["credential_sets"].as_array().map(Vec::len))
            .unwrap_or(0),
    });
    Ok(RegistryRequest {
        signing,
        applying,
        verifying,
        name: request.client_name.chars().take(80).collect(),
        portal: portal.host_str().unwrap_or_default().to_owned(),
        answer_to: uri.host_str().unwrap_or_default().to_owned(),
        purpose: request.purpose,
        did,
    })
}

/// An application's answers as the sheet shows them: label and text by language.
fn answers(content: &serde_json::Value) -> Vec<Answer> {
    let by_language = |value: &serde_json::Value| {
        value
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(lang, text)| {
                        text.as_str()
                            .map(|text| (lang.clone(), text.chars().take(500).collect()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    content["answers"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| Answer {
                    label: by_language(&item["label"]),
                    text: by_language(&item["text"]),
                    verified: item["verified"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Answers the registry request the sheet showed: what Accept does. The
/// request is not read again, so what is signed is what was shown; it is
/// kept until it is answered, so Accept can be tried again after a failure.
#[tauri::command]
pub async fn registry_answer(
    app: tauri::AppHandle,
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
    if request.purpose == "receive" {
        let received = receive(&seed, &uri, &request).await?;
        crate::credentials::keep(&app, &seed, received).map_err(|_| RegistryError::Refused)?;
    } else if request.purpose == "present" || request.purpose == "verify" {
        // The same choice the sheet showed, from the same credentials.
        let (chosen, _) = presentable(&app, &seed, &request);
        if chosen.is_empty() {
            return Err(RegistryError::NothingToPresent);
        }
        let token = crate::presentation::vp_token(
            &chosen,
            |scope| signing_key(&seed, scope, GENERATION),
            &request.client_id,
            &request.nonce,
            now(),
        );
        post(
            &request.response_uri,
            Body::Json(json!({ "vp_token": token })),
        )
        .await?;
    } else {
        // Pairing with an issuer: where it may write to this wallet over
        // DIDComm, when there is a mediator to receive at.
        let channel = match (&request.purpose[..], &request.issuer) {
            ("pair", Some(issuer)) => {
                crate::messaging::issuer_channel(&app, &seed, &issuer.did, &issuer.name).await
            }
            _ => None,
        };
        answer(&seed, &uri, &request, channel.as_deref()).await?;
    }
    if let Ok(mut kept) = shown.0.lock() {
        if kept.as_ref().is_some_and(|(link, _, _)| *link == input) {
            *kept = None;
        }
    }
    Ok(())
}

async fn answer(
    seed: &[u8; 64],
    uri: &Url,
    request: &Request,
    channel: Option<&str>,
) -> Result<(), RegistryError> {
    let (key, did) = identity_for(seed, uri, issuer_of(request));
    match request.purpose.as_str() {
        // Presenting needs the held credentials: `registry_answer` does it.
        // Without them there is nothing to present.
        "present" => return Err(RegistryError::NothingToPresent),
        "submit" => {
            let submission = request
                .submission
                .as_ref()
                .ok_or(RegistryError::Unreadable)?;
            if request.holder.as_deref() != Some(did.as_str()) {
                return Err(RegistryError::NotTheHolder);
            }
            let issued = now();
            let token = id_token(
                &key,
                &did,
                json!({
                    "iss": did,
                    "aud": request.client_id,
                    "nonce": request.nonce,
                    "iat": issued,
                    "exp": issued + TOKEN_LIFETIME,
                    "application": submission.content["application"],
                    "digest": submission.digest,
                }),
            );
            return post(
                &request.response_uri,
                Body::Json(json!({ "signature": token })),
            )
            .await;
        }
        _ => {}
    }
    if let Some(sign) = &request.sign {
        if !sign.signers.iter().any(|signer| signer == multikey(&did)) {
            return Err(RegistryError::NotASigner);
        }
        if sign.kind == "credential" || sign.kind == "status_list" {
            let jws = compact(&key, &sign.document["header"], &sign.document["payload"]);
            return post(&request.response_uri, Body::Json(json!({ "jws": jws }))).await;
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
    let token = id_token(&key, &did, token_claims(&did, request, channel, now()));
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("id_token", &token)
        .finish();
    post(&request.response_uri, Body::Form(body)).await
}

/// The claims of an `id_token` answer. Pairing names, as `didcomm`, the
/// pairwise the issuer may write to (`issuer_channel`); nothing else does.
fn token_claims(
    did: &str,
    request: &Request,
    channel: Option<&str>,
    issued: u64,
) -> serde_json::Value {
    let mut claims = json!({
        "iss": did,
        "sub": did,
        "aud": request.client_id,
        "nonce": request.nonce,
        "iat": issued,
        "exp": issued + TOKEN_LIFETIME,
    });
    if let Some(channel) = channel.filter(|_| request.purpose == "pair") {
        claims["didcomm"] = json!(channel);
    }
    claims
}

/// What collecting answers: the `receive` request to put to the person.
#[derive(Deserialize)]
struct Collected {
    deep_link: String,
}

/// Asks the issuer of an `issued` notice (`messaging/notice.rs`) for the
/// credential: an `id_token` by the key this wallet paired with that issuer —
/// derived again from the `collect` address's origin and the issuer's DID —
/// for that address, with the application's id as nonce. The registry
/// answers with a `receive` request, returned as its `almena://` link for the
/// sheet to read and the person to accept, as the portal's third code would.
#[tauri::command]
pub async fn registry_collect(
    app: tauri::AppHandle,
    held: State<'_, Held>,
    contact: String,
    entry: String,
) -> Result<String, RegistryError> {
    let seed = seed(&held)?;
    let (issuer, notice) = crate::messaging::notice_of(&app, &seed, &contact, &entry)
        .map_err(|_| RegistryError::Unreadable)?;
    let collect = notice.collect.ok_or(RegistryError::Unreadable)?;
    let url = Url::parse(&collect).map_err(|_| RegistryError::Unreadable)?;
    let (key, did) = identity_for(&seed, &url, Some(&issuer));
    let issued = now();
    let token = id_token(
        &key,
        &did,
        json!({
            "iss": did,
            "sub": did,
            "aud": collect,
            "nonce": notice.application,
            "iat": issued,
            "exp": issued + TOKEN_LIFETIME,
        }),
    );
    let response = client()
        .map_err(|_| RegistryError::Unreachable)?
        .post(reachable(&url)?)
        .header("Content-Type", "application/json")
        .body(json!({ "id_token": token }).to_string())
        .send()
        .await
        .map_err(|_| RegistryError::Unreachable)?;
    match response.status().as_u16() {
        200 => {}
        404 | 409 | 410 => return Err(RegistryError::Expired),
        _ => return Err(RegistryError::Refused),
    }
    let collected: Collected = response
        .json()
        .await
        .map_err(|_| RegistryError::Unreadable)?;
    // Only a request on the same registry it was collected from.
    let link = request_uri(&collected.deep_link)?;
    if link.origin() != url.origin() {
        return Err(RegistryError::Unreadable);
    }
    Ok(collected.deep_link)
}

/// A compact JWS, `EdDSA`, over the header and payload given.
fn compact(key: &SigningKey, header: &serde_json::Value, payload: &serde_json::Value) -> String {
    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(payload.to_string())
    );
    let signature = key.sign(signing_input.as_bytes());
    format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    )
}

/// What the registry answers a `receive` with.
#[derive(Deserialize)]
struct Received {
    format: String,
    credential: String,
    issuer: IssuerRef,
    credential_type: CredentialTypeRef,
}

/// Takes the credential the holder applied for: proves the key it is bound to
/// (the one paired with the issuer), and checks what comes back before it is
/// kept — bound to that key, from that issuer, every disclosure listed, and
/// signed by a key the issuer's DID document lists (`issuers.rs`). The
/// document is read first: the registry hands a credential out once, so it is
/// not asked for while the issuer cannot be checked.
async fn receive(
    seed: &[u8; 64],
    uri: &Url,
    request: &Request,
) -> Result<crate::credentials::Credential, RegistryError> {
    let issuer = request.issuer.as_ref().ok_or(RegistryError::Unreadable)?;
    let (key, did) = identity_for(seed, uri, Some(&issuer.did));
    if request.holder.as_deref() != Some(did.as_str()) {
        return Err(RegistryError::NotTheHolder);
    }
    let document =
        crate::issuers::document(&issuer.did)
            .await
            .map_err(|problem| match problem {
                crate::issuers::Unresolved::Unreachable => RegistryError::Unreachable,
                crate::issuers::Unresolved::Unreadable => RegistryError::IssuerUnverified,
            })?;
    let issued = now();
    let token = id_token(
        &key,
        &did,
        json!({
            "iss": did, "sub": did, "aud": request.client_id, "nonce": request.nonce,
            "iat": issued, "exp": issued + TOKEN_LIFETIME,
        }),
    );
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("id_token", &token)
        .finish();
    let answer_to = Url::parse(&request.response_uri).map_err(|_| RegistryError::Unreadable)?;
    let response = client()
        .map_err(|_| RegistryError::Unreachable)?
        .post(reachable(&answer_to)?)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| RegistryError::Unreachable)?;
    match response.status().as_u16() {
        200 => {}
        404 | 409 | 410 => return Err(RegistryError::Expired),
        _ => return Err(RegistryError::Refused),
    }
    let received: Received = response
        .json()
        .await
        .map_err(|_| RegistryError::Unreadable)?;
    if received.format != "dc+sd-jwt" || received.issuer.did != issuer.did {
        return Err(RegistryError::Unreadable);
    }
    let (claims, issued_at, valid_until) =
        crate::credentials::read(&received.credential, &did, &issuer.did, None)
            .map_err(|_| RegistryError::Unreadable)?;
    let jws = received.credential.split('~').next().unwrap_or_default();
    if crate::issuers::signed_by(jws, "dc+sd-jwt", &issuer.did, &document).is_none() {
        return Err(RegistryError::IssuerUnverified);
    }
    Ok(crate::credentials::Credential {
        id: crate::credentials::id_of(&received.credential),
        format: received.format,
        credential: received.credential,
        issuer_did: issuer.did.clone(),
        issuer_name: received.issuer.name.chars().take(200).collect(),
        type_id: received.credential_type.id,
        type_labels: received.credential_type.labels,
        claims,
        issued_at,
        valid_until,
        received_at: now(),
        scope: format!("{} {}", uri.origin().ascii_serialization(), issuer.did),
        checked: crate::status::Checked::default(),
    })
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
pub(crate) fn proof(
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
pub(crate) fn jcs(value: &serde_json::Value) -> Result<String, RegistryError> {
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
            status_change: None,
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
            status_change: None,
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
        let at = |uri: &str| identity_for(&SEED, &Url::parse(uri).unwrap(), None).1;
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
        let (key, did) = identity_for(&SEED, &api(), None);
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
        answer(&SEED, &uri, &request, None).await.unwrap();

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
        let again = answer(&SEED, &uri, &request, None).await;
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
        answer(&SEED, &uri, &request, None).await.unwrap();
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
    fn a_verifier_names_itself_and_its_query_and_nothing_of_an_application() {
        let base = || Request {
            response_type: "vp_token".into(),
            response_mode: "direct_post".into(),
            client_id: "https://registry.almena.id".into(),
            client_name: "Door".into(),
            response_uri: "https://api.almena.id/r".into(),
            nonce: "n".into(),
            purpose: "verify".into(),
            sign: None,
            issuer: None,
            credential_type: None,
            holder: None,
            dcql_query: Some(json!({"credentials": []})),
            submission: None,
            verifier: Some(IssuerRef {
                did: "did:webvh:x:almena.id:ids:door".into(),
                name: "Door".into(),
            }),
            form: None,
        };
        assert!(applies_as_it_says(&base()));
        let no_query = Request {
            dcql_query: None,
            ..base()
        };
        assert!(!applies_as_it_says(&no_query));
        let nameless = Request {
            verifier: Some(IssuerRef {
                did: String::new(),
                name: "Door".into(),
            }),
            ..base()
        };
        assert!(!applies_as_it_says(&nameless));
        let posing = Request {
            issuer: Some(IssuerRef {
                did: "did:web:club".into(),
                name: "Club".into(),
            }),
            ..base()
        };
        assert!(!applies_as_it_says(&posing));
        // A verifier named on anything else does not hold either.
        let signing_in = Request {
            purpose: "sign_in".into(),
            response_type: "id_token".into(),
            dcql_query: None,
            ..base()
        };
        assert!(!applies_as_it_says(&signing_in));
    }

    #[test]
    fn only_pairing_names_where_the_issuer_may_write() {
        let request = |purpose: &str| Request {
            response_type: "id_token".into(),
            response_mode: "direct_post".into(),
            client_id: "https://registry.almena.id".into(),
            client_name: "Club".into(),
            response_uri: "https://api.almena.id/r".into(),
            nonce: "n".into(),
            purpose: purpose.into(),
            sign: None,
            issuer: None,
            credential_type: None,
            holder: None,
            dcql_query: None,
            submission: None,
            verifier: None,
            form: None,
        };
        let paired = token_claims("did:key:z", &request("pair"), Some("did:peer:2.x"), 1);
        assert_eq!(paired["didcomm"], "did:peer:2.x");
        assert_eq!(paired["exp"], 1 + TOKEN_LIFETIME);
        let signed_in = token_claims("did:key:z", &request("sign_in"), Some("did:peer:2.x"), 1);
        assert!(signed_in.get("didcomm").is_none());
        assert!(token_claims("did:key:z", &request("pair"), None, 1)
            .get("didcomm")
            .is_none());
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
        let (key, did) = identity_for(&SEED, &api(), None);
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
        let (key, did) = identity_for(&SEED, &api(), None);
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
        let (key, _) = identity_for(&SEED, &api(), None);
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
        // Siblings under a public suffix belong to different people.
        assert!(!at("https://victim.github.io", "https://evil.github.io/x"));
        assert!(!at("https://a.co.uk", "https://b.co.uk/x"));
        assert!(!at("https://a.vercel.app", "https://b.vercel.app/x"));
        assert!(at(
            "https://registry.example.co.uk",
            "https://api.example.co.uk/x"
        ));
        assert!(!at("http://127.0.0.1:3000", "http://127.0.0.2:8000/x"));
    }

    #[test]
    fn only_web_portals_are_named() {
        assert!(portal("https://registry.almena.id").is_ok());
        assert!(portal("almena://auth").is_err());
        assert!(portal("not a url").is_err());
    }

    fn application(purpose: &str, content: Option<serde_json::Value>) -> Request {
        let submission = content.map(|content| Submission {
            digest: digest_of(&content).unwrap(),
            content,
        });
        Request {
            response_type: match purpose {
                "present" => "vp_token",
                "submit" => "signature",
                _ => "id_token",
            }
            .into(),
            response_mode: "direct_post".into(),
            client_id: "https://registry.almena.id".into(),
            client_name: "Club".into(),
            response_uri: "https://api.almena.id/api/v1/applications/a/request/response".into(),
            nonce: "n".into(),
            purpose: purpose.into(),
            sign: None,
            issuer: Some(IssuerRef {
                did: "did:webvh:Qm:almena.id:ids:idn_club".into(),
                name: "Club".into(),
            }),
            credential_type: Some(CredentialTypeRef {
                id: "membership".into(),
                labels: Default::default(),
            }),
            holder: None,
            dcql_query: (purpose == "present").then(|| json!({"credential_sets": [{}]})),
            submission,
            verifier: None,
            form: None,
        }
    }

    fn content() -> serde_json::Value {
        json!({
            "application": "app_x",
            "issuer": "did:webvh:Qm:almena.id:ids:idn_club",
            "answers": [{"key": "given_name", "label": {"en": "Given name"},
                         "text": {"en": "Lucía"}, "verified": false}],
        })
    }

    #[test]
    fn each_issuer_sees_its_own_did_and_pairing_and_signing_share_it() {
        let club = Some("did:webvh:Qm:almena.id:ids:idn_club");
        let gym = Some("did:webvh:Qm:almena.id:ids:idn_gym");
        let (_, as_registry) = identity_for(&SEED, &api(), None);
        let (_, at_club) = identity_for(&SEED, &api(), club);
        assert_ne!(as_registry, at_club);
        assert_ne!(at_club, identity_for(&SEED, &api(), gym).1);
        assert_eq!(at_club, identity_for(&SEED, &api(), club).1);
        let pair = application("pair", None);
        let submit = application("submit", Some(content()));
        assert_eq!(issuer_of(&pair), issuer_of(&submit));
        assert_eq!(issuer_of(&pair), club);
    }

    #[test]
    fn an_application_holds_together_or_is_refused() {
        assert!(applies_as_it_says(&application("pair", None)));
        assert!(applies_as_it_says(&application("present", None)));
        assert!(applies_as_it_says(&application("submit", Some(content()))));
        // What is shown must be what the digest covers.
        let mut tampered = application("submit", Some(content()));
        tampered.submission.as_mut().unwrap().content["answers"][0]["text"]["en"] = json!("Eve");
        assert!(!applies_as_it_says(&tampered));
        // For another issuer than the one named.
        let mut elsewhere = application("submit", Some(content()));
        elsewhere.issuer.as_mut().unwrap().did = "did:webvh:Qm:almena.id:ids:idn_gym".into();
        assert!(!applies_as_it_says(&elsewhere));
        assert!(!applies_as_it_says(&application("submit", None)));
        let mut unnamed = application("pair", None);
        unnamed.issuer = None;
        assert!(!applies_as_it_says(&unnamed));
        // A sign-in names no issuer.
        let mut sign_in = application("pair", None);
        sign_in.purpose = "sign_in".into();
        assert!(!applies_as_it_says(&sign_in));
        sign_in.issuer = None;
        sign_in.credential_type = None;
        assert!(applies_as_it_says(&sign_in));
    }

    #[test]
    fn the_sheet_reads_the_answers_by_language() {
        let shown = answers(&content());
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].text["en"], "Lucía");
        assert!(!shown[0].verified);
    }

    #[tokio::test]
    async fn nothing_is_presented_and_only_the_paired_key_submits() {
        let present = application("present", None);
        assert!(matches!(
            answer(&SEED, &api(), &present, None).await,
            Err(RegistryError::NothingToPresent)
        ));
        let mut submit = application("submit", Some(content()));
        submit.holder = Some("did:key:z6MkOther".into());
        assert!(matches!(
            answer(&SEED, &api(), &submit, None).await,
            Err(RegistryError::NotTheHolder)
        ));
    }

    fn credential_to_sign() -> Sign {
        let issuer = "did:webvh:Qm:almena.id:ids:idn_club";
        let disclosure =
            URL_SAFE_NO_PAD.encode(json!(["salt", "member_number", "0042"]).to_string());
        let digest = URL_SAFE_NO_PAD.encode(Sha256::digest(disclosure.as_bytes()));
        Sign {
            kind: "credential".into(),
            identity: "Club".into(),
            tenant: Some("Club".into()),
            did: Some(issuer.into()),
            version: None,
            valid_until: Some(timestamp(1_924_991_999)),
            signers: vec!["z6MkSigner".into()],
            verification_method: Some(issuer.into()),
            proof_purpose: "assertionMethod".into(),
            document: json!({
                "header": {"alg": "EdDSA", "typ": "dc+sd-jwt", "kid": format!("{issuer}#z6MkSigner")},
                "payload": {"iss": issuer, "iat": 1_893_456_000u64, "exp": 1_924_991_999u64,
                            "vct": "https://almena.id/credentials/membership/1",
                            "cnf": {"kid": "did:key:zHolder"}, "_sd_alg": "sha-256",
                            "_sd": [digest],
                            "status": {"status_list": {"idx": 7, "uri": "https://api.almena.id/status-lists/club"}}},
                "disclosures": [disclosure],
            }),
            presentation: None,
            status_change: None,
        }
    }

    fn status_list_to_sign(change: Option<StatusChange>) -> Sign {
        let issuer = "did:webvh:Qm:almena.id:ids:idn_club";
        // 2 bits each: entry 5 suspended (bits 2-3 of byte 1), entry 6 revoked.
        let mut list = vec![0u8; 8];
        list[1] = 0b0001_1000;
        let lst = URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec_zlib(&list, 9));
        Sign {
            kind: "status_list".into(),
            identity: "Club".into(),
            tenant: Some("Club".into()),
            did: Some(issuer.into()),
            version: None,
            valid_until: None,
            signers: vec!["z6MkSigner".into()],
            verification_method: Some(issuer.into()),
            proof_purpose: "assertionMethod".into(),
            document: json!({
                "header": {"alg": "EdDSA", "typ": "statuslist+jwt", "kid": format!("{issuer}#z6MkSigner")},
                "payload": {"sub": "https://api.almena.id/status-lists/stl_x", "iat": 1u64,
                            "status_list": {"bits": 2, "lst": lst}},
            }),
            presentation: None,
            status_change: change,
        }
    }

    #[test]
    fn a_status_list_is_signed_only_saying_what_is_shown() {
        let change = |index: u64, status: &str| {
            Some(StatusChange {
                index,
                status: status.into(),
                holder: None,
            })
        };
        assert_eq!(status_at(&status_list_to_sign(None).document, 5), Some(2));
        assert!(told_as_it_is(&status_list_to_sign(None)));
        assert!(told_as_it_is(&status_list_to_sign(change(5, "suspended"))));
        assert!(told_as_it_is(&status_list_to_sign(change(6, "revoked"))));
        assert!(told_as_it_is(&status_list_to_sign(change(4, "valid"))));
        // The sheet would say one thing and the list another.
        assert!(!told_as_it_is(&status_list_to_sign(change(5, "revoked"))));
        assert!(!told_as_it_is(&status_list_to_sign(change(4, "suspended"))));
        assert!(!told_as_it_is(&status_list_to_sign(change(999, "valid"))));
        assert!(!told_as_it_is(&status_list_to_sign(change(5, "expired"))));
        // Another key, another kind of token, a list that does not decode.
        let mut unlisted = status_list_to_sign(None);
        unlisted.signers = vec!["z6MkOther".into()];
        assert!(!told_as_it_is(&unlisted));
        let mut other = status_list_to_sign(None);
        other.document["header"]["typ"] = json!("dc+sd-jwt");
        assert!(!told_as_it_is(&other));
        let mut garbled = status_list_to_sign(None);
        garbled.document["payload"]["status_list"]["lst"] = json!("not-zlib");
        assert!(!told_as_it_is(&garbled));
    }

    #[test]
    fn a_credential_is_signed_only_as_it_is_shown() {
        let sign = credential_to_sign();
        assert!(told_as_it_is(&sign));
        let claims = disclosed(&sign.document).expect("claims");
        assert_eq!(claims, vec![("member_number".to_owned(), json!("0042"))]);
        // A claim the sheet would not show, or one shown that is not signed.
        let mut hidden = credential_to_sign();
        hidden.document["payload"]["_sd"]
            .as_array_mut()
            .unwrap()
            .push(json!("another"));
        assert!(!told_as_it_is(&hidden));
        let mut stray = credential_to_sign();
        stray.document["disclosures"] =
            json!([URL_SAFE_NO_PAD.encode(json!(["salt", "role", "admin"]).to_string())]);
        assert!(!told_as_it_is(&stray));
        // Another issuer's name in the payload, another key, another date.
        let mut elsewhere = credential_to_sign();
        elsewhere.document["payload"]["iss"] = json!("did:web:else");
        assert!(!told_as_it_is(&elsewhere));
        let mut unlisted = credential_to_sign();
        unlisted.signers = vec!["z6MkOther".into()];
        assert!(!told_as_it_is(&unlisted));
        let mut later = credential_to_sign();
        later.valid_until = Some(timestamp(1_924_992_000));
        assert!(!told_as_it_is(&later));
        // Anything signed that the sheet does not show: a claim in the clear,
        // a key bound some other way, a header field more, a list elsewhere.
        let mut clear = credential_to_sign();
        clear.document["payload"]["role"] = json!("admin");
        assert!(!told_as_it_is(&clear));
        let mut bound = credential_to_sign();
        bound.document["payload"]["cnf"] = json!({"jwk": {"kty": "OKP"}});
        assert!(!told_as_it_is(&bound));
        let mut header = credential_to_sign();
        header.document["header"]["jku"] = json!("https://else.example/keys");
        assert!(!told_as_it_is(&header));
        let mut digests = credential_to_sign();
        digests.document["payload"]["_sd_alg"] = json!("sha-512");
        assert!(!told_as_it_is(&digests));
        let mut listed = credential_to_sign();
        listed.document["payload"]["status"]["status_list"]["uri"] =
            json!("http://else.example/list");
        assert!(!told_as_it_is(&listed));
        let mut unstatused = credential_to_sign();
        unstatused.document["payload"]
            .as_object_mut()
            .unwrap()
            .remove("status");
        assert!(told_as_it_is(&unstatused));
    }

    #[test]
    fn the_credential_jws_verifies_over_what_was_sent() {
        let (key, _) = identity_for(&SEED, &api(), None);
        let sign = credential_to_sign();
        let jws = compact(&key, &sign.document["header"], &sign.document["payload"]);
        let mut parts = jws.split('.');
        let header = parts.next().unwrap();
        let payload = parts.next().unwrap();
        let signature = URL_SAFE_NO_PAD.decode(parts.next().unwrap()).unwrap();
        let sent: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        assert_eq!(sent, sign.document["payload"]);
        key.verifying_key()
            .verify(
                format!("{header}.{payload}").as_bytes(),
                &Signature::from_slice(&signature).unwrap(),
            )
            .expect("verifies");
    }
}
