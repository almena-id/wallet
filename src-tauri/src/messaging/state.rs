//! What the wallet writes down about its messaging: which mediator it is
//! connected to, the name it goes by, and who it has a relationship with.
//!
//! **Sealed under a key the seed derives** — `m/2'`, see
//! [`crate::identity::keys::STATE`] — so it is readable exactly while the
//! wallet is open. Which mediator somebody uses is not a secret the way the seed
//! is, but it is the start of the list of relationships this file will hold,
//! and that list is nobody's business but the wallet's.
//!
//! It lives in the application's private directory, is written beside itself
//! and moved into place, and goes when the identity goes: signing out and
//! running out of PIN attempts both call [`clear`]. The conversations are
//! sealed the same way, one file each — see [`super::conversation`].

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime};

use super::MessagingError;
use crate::identity::keys::{self, STATE};

const FILE: &str = "messaging.json";
const VERSION: u32 = 1;
/// Bound to the ciphertext, so a file of another format or version cannot be
/// opened as this one.
const AAD: &[u8] = b"almena-wallet/messaging/1";
const NONCE_BYTES: usize = 24;

/// The mediation this wallet has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mediation {
    /// The mediator's DID.
    pub mediator: String,
    /// The inbox DID registered with it.
    pub inbox: String,
}

/// One relationship: one counterparty, and the pairwise DID this wallet is to
/// it. No secret is kept: the pairwise keys are derived again from the seed and
/// `origin` whenever they are needed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    /// The pairwise DID this wallet speaks as in it.
    pub ours: String,
    /// The DID the counterparty speaks as. Until it answers, that is the
    /// contact card its invitation named.
    pub theirs: String,
    /// The DID the pairwise was derived from: the counterparty as this wallet
    /// first knew it. It does not change when the counterparty rotates.
    pub origin: String,
    /// Waiting for the counterparty's first answer.
    pub pending: bool,
    /// When it was opened, in seconds since the epoch.
    pub since: u64,
    /// The name the counterparty goes by, from its profile.
    #[serde(default)]
    pub name: Option<String>,
    /// The name this wallet gave it, which wins over theirs.
    #[serde(default)]
    pub alias: Option<String>,
    /// Messages that arrived since the conversation was last opened.
    #[serde(default)]
    pub unread: u32,
    /// The latest message either way, so the inbox is drawn without opening
    /// every conversation.
    #[serde(default)]
    pub last: Option<Last>,
    /// Its history was deleted: it stays a contact but leaves the inbox until
    /// something is said in it again.
    #[serde(default)]
    pub cleared: bool,
    /// Opened by somebody who wrote to the card, not by accepting their
    /// invitation.
    #[serde(default)]
    pub welcomed: bool,
}

impl Relationship {
    /// A relationship opened just now, with nothing said in it yet.
    pub fn new(ours: String, theirs: String, pending: bool) -> Self {
        Self {
            ours,
            origin: theirs.clone(),
            theirs,
            pending,
            since: almena_didcomm::message::now(),
            name: None,
            alias: None,
            unread: 0,
            last: None,
            cleared: false,
            welcomed: false,
        }
    }
}

/// The latest message of a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Last {
    pub content: String,
    pub at: u64,
    pub mine: bool,
    /// An issuer's notice: its status (`accepted`, `rejected`, `issued`),
    /// which the inbox words in its own language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// Everything in the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    pub mediation: Option<Mediation>,
    /// The name this wallet goes by, sent to every contact.
    #[serde(default)]
    pub profile: Option<String>,
    /// Added after the first version of the file, which is read as none.
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}

/// The file on disk: its version and the sealed state.
#[derive(Serialize, Deserialize)]
struct Sealed {
    version: u32,
    nonce: String,
    ciphertext: String,
}

/// Reads the state, or an empty one when there is no file.
///
/// # Errors
///
/// [`MessagingError::Unreadable`] when the file does not open under this seed,
/// and [`MessagingError::Storage`] when it cannot be read at all.
pub fn read<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
) -> Result<State, MessagingError> {
    Ok(load(&file(app)?, seed, AAD)?.unwrap_or_default())
}

/// Writes the state, replacing what was there.
///
/// # Errors
///
/// [`MessagingError::Storage`] when it cannot be written, and
/// [`MessagingError::Entropy`] when there is no randomness for the nonce.
pub fn write<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    state: &State,
) -> Result<(), MessagingError> {
    store(&file(app)?, seed, AAD, state)
}

/// Opens the sealed file at `path`, or `None` when there is none.
pub fn load<T: DeserializeOwned>(
    path: &Path,
    seed: &[u8; 64],
    aad: &[u8],
) -> Result<Option<T>, MessagingError> {
    match fs::read(path) {
        Ok(bytes) => open(&bytes, seed, aad).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(MessagingError::Storage),
    }
}

/// Seals `value` into `path`: written beside it and moved into place.
pub fn store<T: Serialize>(
    path: &Path,
    seed: &[u8; 64],
    aad: &[u8],
    value: &T,
) -> Result<(), MessagingError> {
    let bytes = seal(value, seed, aad)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| MessagingError::Storage)?;
    }

    let temporary = path.with_extension("json.writing");
    let mut handle =
        crate::vault::store::create_private(&temporary).map_err(|_| MessagingError::Storage)?;
    handle
        .write_all(&bytes)
        .map_err(|_| MessagingError::Storage)?;
    handle.sync_all().map_err(|_| MessagingError::Storage)?;
    drop(handle);
    fs::rename(&temporary, path).map_err(|_| MessagingError::Storage)
}

/// Removes the file. Called when the identity leaves the device.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(path) = file(app) {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.writing"));
    }
}

pub(crate) fn seal<T: Serialize>(
    value: &T,
    seed: &[u8; 64],
    aad: &[u8],
) -> Result<Vec<u8>, MessagingError> {
    let plaintext = serde_json::to_vec(value).map_err(|_| MessagingError::Storage)?;
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| MessagingError::Entropy)?;

    let key = keys::derive(seed, &[STATE]);
    let ciphertext = XChaCha20Poly1305::new(Key::from_slice(&*key))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad,
            },
        )
        .map_err(|_| MessagingError::Storage)?;

    serde_json::to_vec(&Sealed {
        version: VERSION,
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })
    .map_err(|_| MessagingError::Storage)
}

pub(crate) fn open<T: DeserializeOwned>(
    bytes: &[u8],
    seed: &[u8; 64],
    aad: &[u8],
) -> Result<T, MessagingError> {
    let sealed: Sealed = serde_json::from_slice(bytes).map_err(|_| MessagingError::Unreadable)?;
    if sealed.version != VERSION {
        return Err(MessagingError::Unreadable);
    }
    let nonce = B64
        .decode(&sealed.nonce)
        .ok()
        .filter(|nonce| nonce.len() == NONCE_BYTES)
        .ok_or(MessagingError::Unreadable)?;
    let ciphertext = B64
        .decode(&sealed.ciphertext)
        .map_err(|_| MessagingError::Unreadable)?;

    let key = keys::derive(seed, &[STATE]);
    let plaintext = XChaCha20Poly1305::new(Key::from_slice(&*key))
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map_err(|_| MessagingError::Unreadable)?;

    serde_json::from_slice(&plaintext).map_err(|_| MessagingError::Unreadable)
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, MessagingError> {
    directory(app).map(|dir| dir.join(FILE))
}

/// The application's private directory, where everything here is written.
pub fn directory<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, MessagingError> {
    app.path()
        .app_local_data_dir()
        .map_err(|_| MessagingError::Storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State {
            mediation: Some(Mediation {
                mediator: "did:web:mediator.example.com".into(),
                inbox: "did:peer:2.Vz6Mk".into(),
            }),
            relationships: vec![Relationship {
                ours: "did:peer:2.ours".into(),
                theirs: "did:peer:2.theirs".into(),
                origin: "did:peer:2.card".into(),
                pending: true,
                since: 1,
                name: Some("Alice".into()),
                alias: None,
                unread: 2,
                last: Some(Last {
                    content: "hello".into(),
                    at: 1,
                    mine: false,
                    notice: None,
                }),
                cleared: false,
                welcomed: false,
            }],
            profile: Some("Bob".into()),
        }
    }

    #[test]
    fn what_is_sealed_opens_under_the_same_seed() {
        let seed = [3u8; 64];
        let sealed = seal(&state(), &seed, AAD).expect("sealed");
        assert_eq!(open::<State>(&sealed, &seed, AAD).expect("opened"), state());
    }

    #[test]
    fn another_seed_opens_nothing() {
        let sealed = seal(&state(), &[3u8; 64], AAD).expect("sealed");
        assert!(matches!(
            open::<State>(&sealed, &[4u8; 64], AAD),
            Err(MessagingError::Unreadable)
        ));
    }

    #[test]
    fn the_file_says_nothing_about_what_it_holds() {
        let sealed = seal(&state(), &[3u8; 64], AAD).expect("sealed");
        let text = String::from_utf8(sealed).expect("json");
        assert!(!text.contains("mediator.example.com"));
    }

    #[test]
    fn a_state_written_before_relationships_existed_still_opens() {
        let seed = [3u8; 64];
        let older = serde_json::json!({"mediation": null});
        let key = keys::derive(&seed, &[STATE]);
        let nonce = [0u8; NONCE_BYTES];
        let ciphertext = XChaCha20Poly1305::new(Key::from_slice(&*key))
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: older.to_string().as_bytes(),
                    aad: AAD,
                },
            )
            .expect("sealed");
        let bytes = serde_json::to_vec(&Sealed {
            version: VERSION,
            nonce: B64.encode(nonce),
            ciphertext: B64.encode(ciphertext),
        })
        .expect("json");

        assert_eq!(
            open::<State>(&bytes, &seed, AAD).expect("opened"),
            State::default()
        );
    }

    #[test]
    fn a_file_of_another_version_is_refused() {
        let sealed = seal(&state(), &[3u8; 64], AAD).expect("sealed");
        let mut value: serde_json::Value = serde_json::from_slice(&sealed).expect("json");
        value["version"] = serde_json::json!(VERSION + 1);
        let bytes = serde_json::to_vec(&value).expect("json");
        assert!(matches!(
            open::<State>(&bytes, &[3u8; 64], AAD),
            Err(MessagingError::Unreadable)
        ));
    }
}
