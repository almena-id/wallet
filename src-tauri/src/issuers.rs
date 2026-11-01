//! Knowing what an issuer of an Almena Registry signed: its DID document and
//! the JWS made with one of its keys.
//!
//! A registry issuer's DID is a `did:webvh`: its document is the one its log
//! ends in, fetched over HTTPS (as everything this wallet fetches) and walked
//! entry by entry from the first (`webvh.rs`) — nothing the registry merely
//! says is believed. A JWS is the issuer's only if its `kid` is
//! `{issuer}#{multikey}`, that multikey is one the document lists under
//! `assertionMethod`, and the signature verifies with it (`EdDSA`, ed25519).
//!
//! **A verifier** is checked the same way before the sheet shows it
//! ([`vouch`]): its DID must be a `did:webvh` of the registry the request was
//! read from, its log must hold — which also means it is published, since a
//! draft has no log — and so must the log of the tenant its document names as
//! `controller`. Those logs are signed by the tenant's own update keys, which
//! the registry does not hold, so the registry alone cannot make one up. The
//! domains the tenant's document links to it (`LinkedDomains`) are what the
//! sheet can say about who it is; the verifier's name is in no signed
//! document, and stays the registry's word.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Serialize;
use serde_json::Value;

use crate::messaging::mediator::{client, transport};

/// The ed25519 multicodec prefix of a multikey.
const ED25519_PUB: [u8; 2] = [0xed, 0x01];

/// Why an issuer's document could not be had.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unresolved {
    /// It, or the address it is at, could not be reached.
    Unreachable,
    /// Not a `did:webvh`, or a log that does not hold.
    Unreadable,
}

fn decode(part: &str) -> Option<Value> {
    B64.decode(part)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// The multikeys a DID document lists under `assertionMethod`, by fragment
/// (references, relative or absolute, and embedded methods alike).
fn assertion_keys(document: &Value) -> Vec<String> {
    document["assertionMethod"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.as_str().or_else(|| item["id"].as_str()))
        .filter_map(|id| id.rsplit_once('#').map(|(_, fragment)| fragment.to_owned()))
        .collect()
}

/// Whether the document is the issuer's own.
fn names(document: &Value, issuer: &str) -> bool {
    document["id"].as_str() == Some(issuer)
}

fn ed25519(multikey: &str) -> Option<VerifyingKey> {
    let bytes = bs58::decode(multikey.strip_prefix('z')?).into_vec().ok()?;
    let raw = bytes.strip_prefix(&ED25519_PUB)?;
    VerifyingKey::from_bytes(raw.try_into().ok()?).ok()
}

/// A compact JWS's header and payload, if `issuer` signed it — as `typ` —
/// with a key its `document` lists under `assertionMethod`.
pub fn signed_by(token: &str, typ: &str, issuer: &str, document: &Value) -> Option<(Value, Value)> {
    let mut parts = token.trim().split('.');
    let (Some(header_text), Some(payload_text), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let header = decode(header_text)?;
    let payload = decode(payload_text)?;
    let (signer, key) = header["kid"].as_str()?.rsplit_once('#')?;
    if header["alg"] != "EdDSA"
        || header["typ"] != typ
        || signer != issuer
        || !names(document, issuer)
        || !assertion_keys(document).iter().any(|listed| listed == key)
    {
        return None;
    }
    let signature = Signature::from_slice(&B64.decode(signature).ok()?).ok()?;
    // Strict, as for a log entry's proof (`webvh.rs`).
    ed25519(key)?
        .verify_strict(
            format!("{header_text}.{payload_text}").as_bytes(),
            &signature,
        )
        .ok()?;
    Some((header, payload))
}

/// The largest DID log or status list read: a log grows by an entry with
/// every change of its document, a list by a bit per credential, and neither
/// comes near this.
const BODY_BYTES: usize = 4 * 1024 * 1024;

/// Fetches `url` (HTTPS; loopback HTTP in a debug build) as text, at most
/// [`BODY_BYTES`] of it.
pub async fn get(url: &str, accept: &str) -> Result<String, Unresolved> {
    let address = transport(url).map_err(|_| Unresolved::Unreachable)?;
    let response = client()
        .map_err(|_| Unresolved::Unreachable)?
        .get(address)
        .header("Accept", accept)
        .send()
        .await
        .map_err(|_| Unresolved::Unreachable)?;
    if !response.status().is_success() {
        return Err(Unresolved::Unreachable);
    }
    let body = crate::registry::capped(response, BODY_BYTES)
        .await
        .ok_or(Unresolved::Unreadable)?;
    String::from_utf8(body).map_err(|_| Unresolved::Unreadable)
}

/// The issuer's DID document: the state its `did:webvh` log ends in, once
/// every entry holds.
pub async fn document(issuer: &str) -> Result<Value, Unresolved> {
    let url = crate::webvh::log_url(issuer).ok_or(Unresolved::Unreadable)?;
    let log = get(&url, "text/jsonl, application/jsonl, */*").await?;
    crate::webvh::resolve(&log, issuer).ok_or(Unresolved::Unreadable)
}

/// What this wallet could check about a verifier on its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Vouched {
    /// Its DID and its tenant's both resolve, on the registry asked: see the
    /// module's notes.
    pub verified: bool,
    /// The tenant it belongs to, by DID, once that resolved.
    pub tenant: Option<String>,
    /// The domains the tenant's document links to it.
    pub domains: Vec<String>,
}

/// Whether `did` is a `did:webvh` whose log is on `host`: one of the
/// registry's own identities.
fn on_host(did: &str, host: &str) -> bool {
    crate::webvh::log_url(did)
        .and_then(|url| url::Url::parse(&url).ok())
        .is_some_and(|url| url.host_str() == Some(host))
}

/// The hosts a document's `LinkedDomains` services name.
fn linked_domains(document: &Value) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for service in document["service"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if service["type"].as_str() != Some("LinkedDomains") {
            continue;
        }
        let endpoint = &service["serviceEndpoint"];
        let origins: Vec<&str> = match endpoint.as_str() {
            Some(origin) => vec![origin],
            None => endpoint["origins"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .collect(),
        };
        for origin in origins {
            if let Some(host) = url::Url::parse(origin)
                .ok()
                .filter(|url| url.scheme() == "https")
                .and_then(|url| url.host_str().map(str::to_owned))
            {
                if !found.contains(&host) {
                    found.push(host);
                }
            }
        }
    }
    found
}

/// The tenant a verifier's document names, if it is one of the same registry.
fn controller_of(document: &Value, did: &str, host: &str) -> Option<String> {
    if !names(document, did) {
        return None;
    }
    document["controller"]
        .as_str()
        .filter(|tenant| *tenant != did && on_host(tenant, host))
        .map(str::to_owned)
}

/// Checks the verifier `did`, named by a request read from `host`.
/// Unverified — never an error — when anything does not hold or cannot be
/// reached: the sheet says so and the person decides.
pub async fn vouch(did: &str, host: &str) -> Vouched {
    if !on_host(did, host) {
        return Vouched::default();
    }
    let Ok(verifier) = document(did).await else {
        return Vouched::default();
    };
    let Some(tenant) = controller_of(&verifier, did, host) else {
        return Vouched::default();
    };
    match document(&tenant).await {
        Ok(owner) if names(&owner, &tenant) => Vouched {
            verified: true,
            domains: linked_domains(&owner),
            tenant: Some(tenant),
        },
        _ => Vouched::default(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::identity::keys;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    pub const ISSUER: &str = "did:webvh:QmScid:almena.id:ids:idn_club";

    /// A compact JWS by `key`, naming `kid`.
    pub fn jws(key: &SigningKey, typ: &str, kid: &str, payload: &Value) -> String {
        let header = json!({"alg": "EdDSA", "typ": typ, "kid": kid});
        let input = format!(
            "{}.{}",
            B64.encode(header.to_string()),
            B64.encode(payload.to_string())
        );
        let signature = key.sign(input.as_bytes());
        format!("{input}.{}", B64.encode(signature.to_bytes()))
    }

    /// The issuer's signing key, and its multikey.
    pub fn signer() -> (SigningKey, String) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let multikey = keys::written(&key.verifying_key().to_bytes());
        (key, multikey)
    }

    /// The issuer's document, as its log ends in it.
    pub fn document(multikey: &str) -> Value {
        json!({
            "id": ISSUER,
            "assertionMethod": [format!("{ISSUER}#{multikey}")],
        })
    }

    #[test]
    fn only_a_key_the_issuers_document_lists_signs_for_it() {
        let (key, multikey) = signer();
        let kid = format!("{ISSUER}#{multikey}");
        let doc = document(&multikey);
        let payload = json!({"iss": ISSUER});
        let signed = jws(&key, "dc+sd-jwt", &kid, &payload);
        let (_, read) = signed_by(&signed, "dc+sd-jwt", ISSUER, &doc).expect("signed");
        assert_eq!(read, payload);
        // Another type of token, another issuer, another DID's document.
        assert!(signed_by(&signed, "statuslist+jwt", ISSUER, &doc).is_none());
        assert!(signed_by(&signed, "dc+sd-jwt", "did:webvh:Qm:else", &doc).is_none());
        let mut elsewhere = doc.clone();
        elsewhere["id"] = json!("did:webvh:Qm:else");
        assert!(signed_by(&signed, "dc+sd-jwt", ISSUER, &elsewhere).is_none());
        // A key the document does not list, or one named but not used.
        let stranger = SigningKey::from_bytes(&[9; 32]);
        let other = keys::written(&stranger.verifying_key().to_bytes());
        let unlisted = jws(
            &stranger,
            "dc+sd-jwt",
            &format!("{ISSUER}#{other}"),
            &payload,
        );
        assert!(signed_by(&unlisted, "dc+sd-jwt", ISSUER, &doc).is_none());
        let lying = jws(&stranger, "dc+sd-jwt", &kid, &payload);
        assert!(signed_by(&lying, "dc+sd-jwt", ISSUER, &doc).is_none());
        // Altered after signing.
        let mut parts: Vec<&str> = signed.split('.').collect();
        let altered = B64.encode(json!({"iss": ISSUER, "x": 1}).to_string());
        parts[1] = &altered;
        assert!(signed_by(&parts.join("."), "dc+sd-jwt", ISSUER, &doc).is_none());
    }

    #[test]
    fn a_verifier_is_one_of_the_registry_asked_and_names_its_tenant() {
        const VERIFIER: &str = "did:webvh:QmV:almena.id:ids:idn_door";
        const TENANT: &str = "did:webvh:QmT:almena.id:ids:idn_club";
        assert!(on_host(VERIFIER, "almena.id"));
        assert!(!on_host(VERIFIER, "evil.example"));
        assert!(!on_host("did:key:z6Mk", "almena.id"));

        let document = json!({"id": VERIFIER, "controller": TENANT});
        assert_eq!(
            controller_of(&document, VERIFIER, "almena.id").as_deref(),
            Some(TENANT)
        );
        // Another DID's document, a tenant elsewhere, or itself.
        assert!(controller_of(&document, "did:webvh:QmX:almena.id:ids:x", "almena.id").is_none());
        let elsewhere = json!({"id": VERIFIER, "controller": "did:webvh:QmT:evil.example"});
        assert!(controller_of(&elsewhere, VERIFIER, "almena.id").is_none());
        let own = json!({"id": VERIFIER, "controller": VERIFIER});
        assert!(controller_of(&own, VERIFIER, "almena.id").is_none());
    }

    #[test]
    fn a_tenant_says_which_domains_are_its_own() {
        let one = json!({"service": [
            {"type": "DIDCommMessaging", "serviceEndpoint": {"uri": "https://m.example"}},
            {"type": "LinkedDomains", "serviceEndpoint": "https://club.example"},
        ]});
        assert_eq!(linked_domains(&one), ["club.example"]);
        let several = json!({"service": [{"type": "LinkedDomains", "serviceEndpoint": {
            "origins": ["https://a.example", "http://b.example", "https://a.example"]
        }}]});
        // Only HTTPS origins, each once.
        assert_eq!(linked_domains(&several), ["a.example"]);
        assert!(linked_domains(&json!({})).is_empty());
    }

    #[test]
    fn a_weak_key_signs_nothing() {
        // The identity point as a key, and a signature (`R` the identity, `s`
        // zero) that a lax check accepts over any message whatever.
        let mut point = [0u8; 32];
        point[0] = 1;
        let weak = keys::written(&point);
        let doc = document(&weak);
        let header = json!({"alg": "EdDSA", "typ": "dc+sd-jwt", "kid": format!("{ISSUER}#{weak}")});
        let mut signature = [0u8; 64];
        signature[0] = 1;
        let forged = format!(
            "{}.{}.{}",
            B64.encode(header.to_string()),
            B64.encode(json!({"iss": ISSUER}).to_string()),
            B64.encode(signature)
        );
        assert!(signed_by(&forged, "dc+sd-jwt", ISSUER, &doc).is_none());
    }
}
