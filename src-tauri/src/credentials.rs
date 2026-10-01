//! The credentials this wallet holds: SD-JWT VCs issuers of an Almena
//! Registry granted it (see `registry.rs`, `receive`).
//!
//! **Sealed like the messaging state**, under a key the seed derives and with
//! an associated data of their own, in the application's private directory,
//! one file for all of them; they go when the identity goes.
//!
//! Each is kept whole — the issuer's JWS and every disclosure — with what the
//! wallet read from it to show it: the issuer, the type, the claims, the
//! validity. **Its key is not kept:** a credential is bound (`cnf.kid`) to the
//! `did:key` this wallet derives for that issuer at that registry, which is
//! derived again from the seed and `scope` whenever it is presented.
//!
//! Nothing here checks the issuer's signature: the credential came over HTTPS
//! from the registry the application was made at. What is checked is that it
//! is bound to this wallet's key for that issuer, is the type applied for, and
//! that every disclosure is one the issuer's payload lists.

use std::collections::BTreeMap;
use std::path::PathBuf;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Runtime, State};

use crate::identity::Held;
use crate::messaging::state::{directory, load, store};
use crate::messaging::MessagingError;

const FILE: &str = "credentials.json";
const AAD: &[u8] = b"almena-wallet/credentials/1";

/// One credential, as kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credential {
    /// The SHA-256 of the issuer's JWS, in hex: what tells two apart.
    pub id: String,
    /// `dc+sd-jwt`.
    pub format: String,
    /// The SD-JWT as issued: the JWS, then each disclosure, `~`-separated.
    pub credential: String,
    pub issuer_did: String,
    pub issuer_name: String,
    /// The credential type, by its id in Almena's catalogue, and its names.
    pub type_id: String,
    pub type_labels: BTreeMap<String, String>,
    /// The claims its disclosures carry.
    pub claims: BTreeMap<String, serde_json::Value>,
    /// Seconds since the epoch: when issued, until when, when it arrived.
    pub issued_at: u64,
    pub valid_until: u64,
    pub received_at: u64,
    /// What its key is derived from: the registry's origin and the issuer's DID.
    pub scope: String,
}

/// What a credential sent to this wallet must hold to be kept.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Unreadable,
    NotBound,
    WrongType,
    Disclosure,
}

fn decode(part: &str) -> Option<serde_json::Value> {
    B64.decode(part)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// Reads an issued SD-JWT: bound to `holder`, of `vct`, from `issuer`, every
/// disclosure listed in its `_sd`. Its claims, and its `iat` and `exp`.
pub fn read(
    credential: &str,
    holder: &str,
    issuer: &str,
    vct: Option<&str>,
) -> Result<(BTreeMap<String, serde_json::Value>, u64, u64), Refusal> {
    let mut parts = credential.split('~');
    let jws = parts.next().ok_or(Refusal::Unreadable)?;
    let payload = jws
        .split('.')
        .nth(1)
        .and_then(decode)
        .ok_or(Refusal::Unreadable)?;
    if payload["cnf"]["kid"].as_str() != Some(holder) {
        return Err(Refusal::NotBound);
    }
    if payload["iss"].as_str() != Some(issuer)
        || vct.is_some_and(|vct| payload["vct"].as_str() != Some(vct))
    {
        return Err(Refusal::WrongType);
    }
    let listed: Vec<&str> = payload["_sd"]
        .as_array()
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    let mut claims = BTreeMap::new();
    for disclosure in parts.filter(|part| !part.is_empty()) {
        let digest = B64.encode(Sha256::digest(disclosure.as_bytes()));
        let decoded = decode(disclosure).ok_or(Refusal::Disclosure)?;
        let (Some(name), Some(value)) = (decoded[1].as_str(), decoded.get(2)) else {
            return Err(Refusal::Disclosure);
        };
        if !listed.contains(&digest.as_str()) || claims.contains_key(name) {
            return Err(Refusal::Disclosure);
        }
        claims.insert(name.to_owned(), value.clone());
    }
    let at = |name: &str| payload[name].as_u64().unwrap_or(0);
    Ok((claims, at("iat"), at("exp")))
}

/// The SHA-256 of a credential's JWS, in hex.
pub fn id_of(credential: &str) -> String {
    let jws = credential.split('~').next().unwrap_or_default();
    Sha256::digest(jws.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, MessagingError> {
    Ok(directory(app)?.join(FILE))
}

pub fn all<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
) -> Result<Vec<Credential>, MessagingError> {
    Ok(load(&file(app)?, seed, AAD)?.unwrap_or_default())
}

/// Keeps a credential, replacing one with the same JWS.
pub fn keep<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    credential: Credential,
) -> Result<(), MessagingError> {
    let mut held = all(app, seed)?;
    held.retain(|kept| kept.id != credential.id);
    held.push(credential);
    store(&file(app)?, seed, AAD, &held)
}

/// Forgets them all. Called when the identity leaves the device.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(path) = file(app) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.writing"));
    }
}

/// The credentials, newest first, for the Credentials screen.
#[tauri::command]
pub fn credentials_list<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Vec<Credential>, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let mut list = all(&app, &seed)?;
    list.sort_by_key(|credential| std::cmp::Reverse(credential.received_at));
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn b64(value: &serde_json::Value) -> String {
        B64.encode(value.to_string())
    }

    fn issued(holder: &str, extra: Option<&str>) -> String {
        let disclosure = b64(&json!(["salt", "member_number", "0042"]));
        let digest = B64.encode(Sha256::digest(disclosure.as_bytes()));
        let payload = json!({
            "iss": "did:webvh:Qm:almena.id:ids:idn_club",
            "vct": "https://almena.id/credentials/membership/v1",
            "cnf": {"kid": holder},
            "_sd": [digest],
            "iat": 1, "exp": 2,
        });
        let jws = format!("{}.{}.sig", b64(&json!({"alg": "EdDSA"})), b64(&payload));
        match extra {
            Some(more) => format!("{jws}~{disclosure}~{more}~"),
            None => format!("{jws}~{disclosure}~"),
        }
    }

    #[test]
    fn a_credential_is_kept_only_if_it_is_bound_to_this_wallet_and_holds_together() {
        let issuer = "did:webvh:Qm:almena.id:ids:idn_club";
        let vct = Some("https://almena.id/credentials/membership/v1");
        let (claims, iat, exp) =
            read(&issued("did:key:zMe", None), "did:key:zMe", issuer, vct).expect("read");
        assert_eq!(claims["member_number"], json!("0042"));
        assert_eq!((iat, exp), (1, 2));
        assert_eq!(
            read(&issued("did:key:zOther", None), "did:key:zMe", issuer, vct),
            Err(Refusal::NotBound)
        );
        assert_eq!(
            read(
                &issued("did:key:zMe", None),
                "did:key:zMe",
                "did:web:else",
                vct
            ),
            Err(Refusal::WrongType)
        );
        let stray = b64(&json!(["salt", "role", "admin"]));
        assert_eq!(
            read(
                &issued("did:key:zMe", Some(&stray)),
                "did:key:zMe",
                issuer,
                vct
            ),
            Err(Refusal::Disclosure)
        );
    }
}
