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
//! Each one's status — valid, suspended or revoked — is read from its
//! issuer's status list when the Credentials screen asks
//! (`credentials_check`), and kept with it (`status.rs`).
//!
//! `read` checks that a credential is bound to this wallet's key for that
//! issuer, is the type applied for, and that every disclosure is one the
//! issuer's payload lists; the issuer's signature is checked against its DID
//! document (`issuers.rs`) when it is received (`registry.rs`) and again with
//! each status check, since a key the issuer drops leaves it unverifiable.

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
use crate::status::{self, Checked};

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
    /// Its status, as its issuer's status list last said it (`status.rs`).
    #[serde(default)]
    pub checked: Checked,
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
    // Without an `exp` it would be kept as expired in 1970 and never be
    // presentable; the digests are SHA-256, as `_sd` is checked with.
    if payload["exp"].as_u64().is_none()
        || !matches!(
            payload.get("_sd_alg").map(|alg| alg.as_str()),
            None | Some(Some("sha-256"))
        )
    {
        return Err(Refusal::Unreadable);
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

/// Writes the credentials held as `list`, whatever they were: for a restore
/// (`messaging::backup`), which merged them first.
pub fn replace<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    list: &[Credential],
) -> Result<(), MessagingError> {
    store(&file(app)?, seed, AAD, &list)
}

/// `held` without the credential `id`, or `None` when it holds no such one.
fn without(mut held: Vec<Credential>, id: &str) -> Option<Vec<Credential>> {
    let before = held.len();
    held.retain(|kept| kept.id != id);
    (held.len() < before).then_some(held)
}

/// Removes one credential from the wallet, for good: nothing keeps a copy, and
/// only its issuer can grant it again. Returns the rest, as `credentials_list`
/// does.
#[tauri::command]
pub fn credentials_remove<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    id: String,
) -> Result<Vec<Credential>, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let all = all(&app, &seed)?;
    let mut list = match without(all.clone(), &id) {
        Some(rest) => {
            store(&file(&app)?, &seed, AAD, &rest)?;
            rest
        }
        // Already gone: the list as it is.
        None => all,
    };
    list.sort_by_key(|credential| std::cmp::Reverse(credential.received_at));
    Ok(list)
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

/// Checks every credential's status with its issuer's list, keeps what was
/// learnt, and returns them as `credentials_list` does.
#[tauri::command]
pub async fn credentials_check<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Vec<Credential>, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let mut learnt = Vec::new();
    for credential in all(&app, &seed)? {
        let outcome = status::check(&credential.credential, &credential.issuer_did).await;
        learnt.push((
            credential.id.clone(),
            credential.checked.after(outcome, status::now()),
        ));
    }
    // Read again before writing: one received meanwhile is kept.
    let mut list = all(&app, &seed)?;
    for credential in &mut list {
        if let Some((_, checked)) = learnt.iter().find(|(id, _)| *id == credential.id) {
            credential.checked = checked.clone();
        }
    }
    store(&file(&app)?, &seed, AAD, &list)?;
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
    fn removing_takes_out_that_credential_only() {
        let one = |id: &str| Credential {
            id: id.into(),
            format: "dc+sd-jwt".into(),
            credential: String::new(),
            issuer_did: "did:web:issuer".into(),
            issuer_name: "Issuer".into(),
            type_id: "membership".into(),
            type_labels: BTreeMap::new(),
            claims: BTreeMap::new(),
            issued_at: 1,
            valid_until: 2,
            received_at: 1,
            scope: String::new(),
            checked: Checked::default(),
        };
        let rest = without(vec![one("a"), one("b")], "a").expect("held");
        assert_eq!(rest, vec![one("b")]);
        assert_eq!(without(rest, "a"), None);
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
        // No `exp`, or digests of another algorithm than the one checked.
        let reissued = |change: &dyn Fn(&mut serde_json::Value)| {
            let (head, rest) = issued("did:key:zMe", None)
                .split_once('.')
                .map(|(h, r)| (h.to_owned(), r.to_owned()))
                .unwrap();
            let (body, tail) = rest.split_once('.').unwrap();
            let mut payload: serde_json::Value =
                serde_json::from_slice(&B64.decode(body).unwrap()).unwrap();
            change(&mut payload);
            format!("{head}.{}.{tail}", b64(&payload))
        };
        assert_eq!(
            read(
                &reissued(&|p| {
                    p.as_object_mut().unwrap().remove("exp");
                }),
                "did:key:zMe",
                issuer,
                vct
            ),
            Err(Refusal::Unreadable)
        );
        assert_eq!(
            read(
                &reissued(&|p| p["_sd_alg"] = json!("sha-512")),
                "did:key:zMe",
                issuer,
                vct
            ),
            Err(Refusal::Unreadable)
        );
        assert!(read(
            &reissued(&|p| p["_sd_alg"] = json!("sha-256")),
            "did:key:zMe",
            issuer,
            vct
        )
        .is_ok());
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
