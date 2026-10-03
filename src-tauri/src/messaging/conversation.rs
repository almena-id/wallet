//! What was said in each relationship, one sealed file per conversation.
//!
//! **One file each, not one for everything**, so a message that arrives
//! rewrites its own conversation and nothing else, and opening the inbox never
//! reads a history. The files are sealed under the same key as the state
//! ([`super::state`]), and each is bound to its conversation's id, so one
//! cannot be passed off as another.
//!
//! A conversation's id is a hash of the pairwise DID this wallet speaks as in
//! it: stable, the same length for every file name, and it says nothing about
//! the DID without the DID.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::Runtime;

use super::state;
use super::MessagingError;

const DIRECTORY: &str = "conversations";
/// Followed by the conversation's id.
const AAD: &str = "almena-wallet/conversation/1/";

/// One message, either way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The DIDComm message's `id`, which is what a second delivery of the same
    /// message is recognised by.
    pub id: String,
    /// Sent by this wallet.
    pub mine: bool,
    pub content: String,
    /// When it was written, in seconds since the epoch: the sender's
    /// `created_time` for what arrived.
    pub at: u64,
    /// Sent by this wallet and not taken by the other side's mediator.
    #[serde(default)]
    pub failed: bool,
    /// An issuer's notice about an application (`notice.rs`): drawn from
    /// this, translated, rather than from `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<super::notice::Notice>,
}

/// The id of the conversation this wallet has as `ours`.
pub fn id(ours: &str) -> String {
    Sha256::digest(ours.as_bytes())[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The conversation `id`, oldest first; empty when nothing has been said.
pub fn read<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    id: &str,
) -> Result<Vec<Entry>, MessagingError> {
    Ok(state::load(&file(app, id)?, seed, aad(id).as_bytes())?.unwrap_or_default())
}

/// Adds `entry`, or replaces the one with its `id`. Returns whether it was new.
pub fn put<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    id: &str,
    entry: Entry,
) -> Result<bool, MessagingError> {
    let mut entries = read(app, seed, id)?;
    let new = match entries.iter_mut().find(|e| e.id == entry.id) {
        // A second delivery of something already here changes nothing; a
        // retried message of this wallet's own does.
        Some(existing) if existing.mine => {
            *existing = entry;
            false
        }
        Some(_) => return Ok(false),
        None => {
            entries.push(entry);
            true
        }
    };
    entries.sort_by_key(|e| e.at);
    state::store(&file(app, id)?, seed, aad(id).as_bytes(), &entries)?;
    Ok(new)
}

/// Writes the conversation `id` as `entries`, whatever it held: for a
/// restore (`backup.rs`), which merged them first.
pub fn replace<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    id: &str,
    entries: &[Entry],
) -> Result<(), MessagingError> {
    state::store(&file(app, id)?, seed, aad(id).as_bytes(), &entries)
}

/// Removes the conversation `id`'s history.
pub fn remove<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) -> Result<(), MessagingError> {
    let path = file(app, id)?;
    for path in [path.with_extension("json.writing"), path] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(MessagingError::Storage),
        }
    }
    Ok(())
}

/// Removes every conversation. Called when the identity leaves the device.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(dir) = state::directory(app) {
        let _ = fs::remove_dir_all(dir.join(DIRECTORY));
    }
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) -> Result<PathBuf, MessagingError> {
    // Only ever a hash this module made; anything else is not a file name.
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(MessagingError::ContactUnknown);
    }
    Ok(state::directory(app)?
        .join(DIRECTORY)
        .join(format!("{id}.json")))
}

fn aad(id: &str) -> String {
    format!("{AAD}{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_a_stable_hex_hash() {
        assert_eq!(id("did:peer:2.a"), id("did:peer:2.a"));
        assert_ne!(id("did:peer:2.a"), id("did:peer:2.b"));
        assert_eq!(id("did:peer:2.a").len(), 32);
    }
}
