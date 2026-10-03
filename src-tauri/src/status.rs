//! Whether a credential this wallet holds still holds: its issuer's signature
//! on it, and its entry in its issuer's status list (IETF Token Status List).
//!
//! An SD-JWT VC from an Almena Registry names its entry (`status.status_list`:
//! `idx` and `uri`); the list is a JWT (`typ: statuslist+jwt`) the issuer's
//! signer signed, packing `bits` per credential — `0` valid, `1` revoked,
//! `2` suspended — zlib-compressed and base64url-encoded.
//!
//! **Nothing is believed unchecked.** The issuer's DID document is resolved
//! from its `did:webvh` log (`issuers.rs`, `webvh.rs`) and the credential must still be signed by a key it lists —
//! as verifiers will require. The list is fetched from its `uri` (HTTPS, as
//! every request this wallet makes) and must be the issuer's too, by the same
//! rule; it must say it is this one (`sub`) and, if it says until when it
//! holds (`exp`), still hold. A check that fails any of this gives no status:
//! the credential is shown as not checked, never as valid.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::issuers::{self, Unresolved};

/// A credential's status, as its issuer's list says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Valid,
    Revoked,
    Suspended,
}

impl Status {
    fn of(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Valid),
            1 => Some(Self::Revoked),
            2 => Some(Self::Suspended),
            _ => None,
        }
    }
}

/// Why no status could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Unchecked {
    /// The credential names no status list.
    NoList,
    /// The list or the issuer's DID document could not be fetched.
    Unreachable,
    /// The list is not one the issuer signed, or does not hold together.
    Untrusted,
    /// The credential is not signed by a key its issuer's DID lists (any
    /// more): verifiers will refuse it.
    Signature,
}

/// What the wallet last learnt of a credential's status: the status its list
/// gave and when, and why the last check gave none, if it did not.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub status: Option<Status>,
    /// Seconds since the epoch.
    pub at: Option<u64>,
    pub problem: Option<Unchecked>,
}

impl Checked {
    /// After a check: a status replaces what was known; a list that could not
    /// be reached leaves the last status known (with when it was read); one
    /// that cannot be trusted, or none, leaves no status.
    pub fn after(&self, outcome: Result<Status, Unchecked>, at: u64) -> Self {
        match outcome {
            Ok(status) => Self {
                status: Some(status),
                at: Some(at),
                problem: None,
            },
            Err(Unchecked::Unreachable) => Self {
                problem: Some(Unchecked::Unreachable),
                ..self.clone()
            },
            Err(problem) => Self {
                problem: Some(problem),
                ..Self::default()
            },
        }
    }
}

fn decode(part: &str) -> Option<Value> {
    B64.decode(part)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The status at `index` of a Status List Token's payload: its `lst`,
/// base64url of the zlib-compressed list, `bits` per status from the least
/// significant bits of each byte up.
pub fn status_at(payload: &Value, index: u64) -> Option<u8> {
    let list = &payload["status_list"];
    let bits = list["bits"]
        .as_u64()
        .filter(|bits| matches!(bits, 1 | 2 | 4 | 8))?;
    let packed = B64.decode(list["lst"].as_str()?).ok()?;
    let data = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, 1 << 20).ok()?;
    let per_byte = 8 / bits;
    let byte = *data.get(usize::try_from(index / per_byte).ok()?)?;
    let shift = (index % per_byte) * bits;
    Some((byte >> shift) & ((1u16 << bits) - 1) as u8)
}

/// Where a credential's status is: its list's address and its index there.
pub fn entry(credential: &str) -> Option<(String, u64)> {
    let jws = credential.split('~').next()?;
    let payload = decode(jws.split('.').nth(1)?)?;
    let reference = &payload["status"]["status_list"];
    Some((
        reference["uri"].as_str()?.to_owned(),
        reference["idx"].as_u64()?,
    ))
}

/// What the list says of the entry at `index`, if `token` is the list at
/// `uri` as signed by `issuer` with a key its DID `document` lists.
pub fn read(
    token: &str,
    uri: &str,
    index: u64,
    issuer: &str,
    document: &Value,
    at: u64,
) -> Result<Status, Unchecked> {
    let (_, payload) = issuers::signed_by(token, "statuslist+jwt", issuer, document)
        .ok_or(Unchecked::Untrusted)?;
    if payload["sub"].as_str() != Some(uri) || payload["exp"].as_u64().is_some_and(|exp| exp <= at)
    {
        return Err(Unchecked::Untrusted);
    }
    status_at(&payload, index)
        .and_then(Status::of)
        .ok_or(Unchecked::Untrusted)
}

fn unchecked(problem: Unresolved) -> Unchecked {
    match problem {
        Unresolved::Unreachable => Unchecked::Unreachable,
        Unresolved::Unreadable => Unchecked::Untrusted,
    }
}

/// The status of a credential from `issuer`, fetched and checked now: its
/// signature first, then its entry in the list.
pub async fn check(credential: &str, issuer: &str) -> Result<Status, Unchecked> {
    let document = issuers::document(issuer).await.map_err(unchecked)?;
    let jws = credential.split('~').next().unwrap_or_default();
    if issuers::signed_by(jws, "dc+sd-jwt", issuer, &document).is_none() {
        return Err(Unchecked::Signature);
    }
    let (uri, index) = entry(credential).ok_or(Unchecked::NoList)?;
    let token = issuers::get(&uri, "application/statuslist+jwt")
        .await
        .map_err(unchecked)?;
    read(&token, &uri, index, issuer, &document, now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::keys;
    use crate::issuers::tests::{document, jws, signer, ISSUER};
    use ed25519_dalek::SigningKey;
    use serde_json::json;

    const URI: &str = "https://api.almena.id/status-lists/stl_x";

    fn list(statuses: &[(u64, u8)]) -> String {
        let mut data = vec![0u8; 8];
        for &(index, value) in statuses {
            let at = usize::try_from(index / 4).unwrap();
            data[at] |= value << ((index % 4) * 2);
        }
        B64.encode(miniz_oxide::deflate::compress_to_vec_zlib(&data, 9))
    }

    fn token(key: &SigningKey, kid: &str, payload: &Value) -> String {
        jws(key, "statuslist+jwt", kid, payload)
    }

    #[test]
    fn a_signed_list_says_each_entrys_status() {
        let (key, multikey) = signer();
        let payload = json!({"sub": URI, "iat": 1, "status_list": {"bits": 2, "lst": list(&[(5, 2), (6, 1)])}});
        let signed = token(&key, &format!("{ISSUER}#{multikey}"), &payload);
        let doc = document(&multikey);
        assert_eq!(read(&signed, URI, 4, ISSUER, &doc, 10), Ok(Status::Valid));
        assert_eq!(
            read(&signed, URI, 5, ISSUER, &doc, 10),
            Ok(Status::Suspended)
        );
        assert_eq!(read(&signed, URI, 6, ISSUER, &doc, 10), Ok(Status::Revoked));
        // Past its end, the entry is not there.
        assert_eq!(
            read(&signed, URI, 999, ISSUER, &doc, 10),
            Err(Unchecked::Untrusted)
        );
    }

    #[test]
    fn a_list_the_issuer_did_not_sign_says_nothing() {
        let (key, multikey) = signer();
        let payload =
            json!({"sub": URI, "iat": 1, "exp": 20, "status_list": {"bits": 2, "lst": list(&[])}});
        let kid = format!("{ISSUER}#{multikey}");
        let doc = document(&multikey);
        // Another DID's document.
        let mut elsewhere = doc.clone();
        elsewhere["id"] = json!("did:webvh:Qm:else");
        let signed = token(&key, &kid, &payload);
        assert_eq!(
            read(&signed, URI, 0, ISSUER, &elsewhere, 10),
            Err(Unchecked::Untrusted)
        );
        // A key the issuer's DID does not list.
        let stranger = SigningKey::from_bytes(&[9; 32]);
        let other = keys::written(&stranger.verifying_key().to_bytes());
        let forged = token(&stranger, &format!("{ISSUER}#{other}"), &payload);
        assert_eq!(
            read(&forged, URI, 0, ISSUER, &doc, 10),
            Err(Unchecked::Untrusted)
        );
        // Signed by another key than the one it names.
        let lying = token(&stranger, &kid, &payload);
        assert_eq!(
            read(&lying, URI, 0, ISSUER, &doc, 10),
            Err(Unchecked::Untrusted)
        );
        // Another issuer's, another list's, or no longer in force.
        let signed = token(&key, &kid, &payload);
        assert_eq!(
            read(&signed, URI, 0, "did:webvh:Qm:else", &doc, 10),
            Err(Unchecked::Untrusted)
        );
        assert_eq!(
            read(&signed, "https://else/list", 0, ISSUER, &doc, 10),
            Err(Unchecked::Untrusted)
        );
        assert_eq!(
            read(&signed, URI, 0, ISSUER, &doc, 30),
            Err(Unchecked::Untrusted)
        );
        assert_eq!(read(&signed, URI, 0, ISSUER, &doc, 10), Ok(Status::Valid));
    }

    #[test]
    fn an_unreachable_list_keeps_the_last_status_and_an_untrusted_one_none() {
        let known = Checked::default().after(Ok(Status::Suspended), 5);
        assert_eq!(known.status, Some(Status::Suspended));
        let offline = known.after(Err(Unchecked::Unreachable), 9);
        assert_eq!(
            (offline.status, offline.at),
            (Some(Status::Suspended), Some(5))
        );
        assert_eq!(offline.problem, Some(Unchecked::Unreachable));
        let forged = offline.after(Err(Unchecked::Untrusted), 10);
        assert_eq!(
            (forged.status, forged.problem),
            (None, Some(Unchecked::Untrusted))
        );
    }

    #[test]
    fn a_credential_names_its_entry() {
        let payload = json!({"status": {"status_list": {"idx": 42, "uri": URI}}});
        let credential = format!("e30.{}.sig~d~", B64.encode(payload.to_string()));
        assert_eq!(entry(&credential), Some((URI.to_owned(), 42)));
        assert_eq!(entry("e30.e30.sig~"), None);
    }
}
