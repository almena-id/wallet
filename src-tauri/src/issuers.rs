//! Knowing what an issuer of an Almena Registry signed: its DID document and
//! the JWS made with one of its keys.
//!
//! A registry issuer's DID is a `did:webvh`: its document is the one its log
//! ends in, fetched over HTTPS (as everything this wallet fetches) and walked
//! entry by entry from the first (`webvh.rs`) — nothing the registry merely
//! says is believed. A JWS is the issuer's only if its `kid` is
//! `{issuer}#{multikey}`, that multikey is one the document lists under
//! `assertionMethod`, and the signature verifies with it (`EdDSA`, ed25519).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
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
    ed25519(key)?
        .verify(
            format!("{header_text}.{payload_text}").as_bytes(),
            &signature,
        )
        .ok()?;
    Some((header, payload))
}

/// Fetches `url` (HTTPS; loopback HTTP in a debug build) as text.
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
    response.text().await.map_err(|_| Unresolved::Unreachable)
}

/// The issuer's DID document: the state its `did:webvh` log ends in, once
/// every entry holds.
pub async fn document(issuer: &str) -> Result<Value, Unresolved> {
    let url = crate::webvh::log_url(issuer).ok_or(Unresolved::Unreadable)?;
    let log = get(&url, "text/jsonl, application/jsonl, */*").await?;
    crate::webvh::resolve(&log, issuer).ok_or(Unresolved::Unreadable)
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
}
