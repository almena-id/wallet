//! What this wallet has presented, to whom and when: a log the holder reads,
//! kept on the device alone.
//!
//! **Sealed like the credentials** (`credentials.rs`), under the key the seed
//! derives and an associated data of its own, in one file; it goes when the
//! identity goes and travels in a backup. An entry is written only once a
//! presentation was accepted by the registry it was sent to (`registry.rs`),
//! and says who asked (as the sheet showed it, and whether the wallet could
//! check that), when, and which credentials went with **the names** of the
//! claims disclosed — not their values, which the credentials themselves hold.
//!
//! The newest [`KEPT`] are kept; older ones are let go.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{Runtime, State};

use crate::identity::Held;
use crate::messaging::state::{directory, load, store};
use crate::messaging::MessagingError;

const FILE: &str = "presented.json";
const AAD: &[u8] = b"almena-wallet/presented/1";

/// How many entries are kept, newest first.
pub const KEPT: usize = 500;

/// One presentation, as the log keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    /// What tells two entries apart: the request's nonce and the time.
    pub id: String,
    /// Seconds since the epoch.
    pub at: u64,
    /// `present` (to an issuer, applying) or `verify` (to a verifier).
    pub purpose: String,
    /// Who it went to, as the sheet named them, and their DID.
    pub to: String,
    pub to_did: String,
    /// `verify`: whether the wallet checked the verifier itself
    /// (`issuers::vouch`); `None` where nothing was checked.
    #[serde(default)]
    pub verified: Option<bool>,
    /// `verify`: the domains its tenant's document links to it.
    #[serde(default)]
    pub domains: Vec<String>,
    /// The registry portal it went through.
    pub portal: String,
    pub credentials: Vec<Shared>,
}

/// One credential of a presentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shared {
    /// The held credential's id (`credentials::Credential::id`).
    pub credential: String,
    pub type_labels: BTreeMap<String, String>,
    pub issuer_name: String,
    /// The claims disclosed, by name.
    pub claims: Vec<String>,
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, MessagingError> {
    Ok(directory(app)?.join(FILE))
}

/// Every entry, newest first.
pub fn all<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
) -> Result<Vec<Presentation>, MessagingError> {
    Ok(load(&file(app)?, seed, AAD)?.unwrap_or_default())
}

/// `log` with `entry` in front, at most [`KEPT`] long; an entry already there
/// (the same id) is not written twice.
fn with(mut log: Vec<Presentation>, entry: Presentation) -> Vec<Presentation> {
    if log.iter().any(|kept| kept.id == entry.id) {
        return log;
    }
    log.insert(0, entry);
    log.sort_by_key(|kept| std::cmp::Reverse(kept.at));
    log.truncate(KEPT);
    log
}

/// Writes down one presentation.
pub fn record<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    entry: Presentation,
) -> Result<(), MessagingError> {
    let log = with(all(app, seed)?, entry);
    store(&file(app)?, seed, AAD, &log)
}

/// Writes the log as `list`, whatever it was: for a restore
/// (`messaging::backup`), which merged it first.
pub fn replace<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    list: Vec<Presentation>,
) -> Result<(), MessagingError> {
    let log = list.into_iter().fold(Vec::new(), with);
    store(&file(app)?, seed, AAD, &log)
}

/// What has been presented, newest first.
#[tauri::command]
pub fn presented_list<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Vec<Presentation>, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    all(&app, &seed)
}

/// Forgets the log. Called when the identity leaves the device.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(path) = file(app) {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json.writing"));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn entry(id: &str, at: u64) -> Presentation {
        Presentation {
            id: id.into(),
            at,
            purpose: "verify".into(),
            to: "Door".into(),
            to_did: "did:webvh:Qm:almena.id:ids:idn_door".into(),
            verified: Some(true),
            domains: vec!["club.example".into()],
            portal: "registry.almena.id".into(),
            credentials: vec![Shared {
                credential: "x".into(),
                type_labels: BTreeMap::new(),
                issuer_name: "Club".into(),
                claims: vec!["member_number".into()],
            }],
        }
    }

    #[test]
    fn the_newest_come_first_once_each_and_only_so_many() {
        let log = with(with(Vec::new(), entry("a", 1)), entry("b", 2));
        assert_eq!(
            log.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        // The same one again changes nothing.
        assert_eq!(with(log.clone(), entry("a", 1)), log);

        let full =
            (0..KEPT as u64 + 3).fold(Vec::new(), |log, at| with(log, entry(&at.to_string(), at)));
        assert_eq!(full.len(), KEPT);
        assert_eq!(full[0].at, KEPT as u64 + 2);
    }

    #[test]
    fn an_entry_reads_back_as_written_and_older_ones_without_the_new_fields() {
        let written = serde_json::to_value(entry("a", 1)).unwrap();
        let read: Presentation = serde_json::from_value(written.clone()).unwrap();
        assert_eq!(read, entry("a", 1));
        let mut older = written;
        older.as_object_mut().unwrap().remove("verified");
        older.as_object_mut().unwrap().remove("domains");
        let read: Presentation = serde_json::from_value(older).unwrap();
        assert!(read.verified.is_none() && read.domains.is_empty());
    }
}
