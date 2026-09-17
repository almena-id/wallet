//! What the wallet writes down about its relationships: who each one is
//! with, which pairwise it is, which mediator it collects from, and the
//! messages that came through it.
//!
//! **This is the association the platform must never hold**, in one file, and
//! it is sealed under a key derived from the seed — `m/2'`, see
//! [`crate::identity::keys::BOOK`] — so that it is readable exactly when the
//! wallet is open and not otherwise. Locking the wallet drops the seed, and
//! with it the only way to read this; there is no second copy of the key
//! anywhere. A backup that carries the file carries nothing without the words.
//!
//! It is read, changed and written back whole, on every command. There is no
//! cache to keep in step with the disk and nothing left in memory to forget
//! when the wallet locks; a person's relationships are dozens, and dozens of
//! rows is not a workload.
//!
//! **Rows carry data, not their own history.** A message keeps the time its
//! sender put in it, which is the message's; nothing here records when the
//! wallet wrote a row.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Manager, Runtime};
use zeroize::Zeroizing;

use super::invite::Invite;
use super::request::Summary;
use super::MessagingError;
use crate::identity::keys;

const FILE: &str = "messaging.json";
const VERSION: u32 = 1;
/// Bound to the ciphertext so a sealed book of another format cannot be
/// opened as this one.
const AAD: &[u8] = b"almena-id/messaging/v1";
const NONCE_BYTES: usize = 24;

/// One relationship: one counterparty, one pairwise, one mailbox.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    /// The DID the relationship is with, which is what the pairwise is
    /// derived from.
    pub counterparty: String,
    /// The `did:key` this wallet is in this relationship.
    pub pairwise: String,
    /// The mediator whose mailbox this pairwise collects from: the one that
    /// was chosen when the relationship was opened, kept here because a
    /// later change of preference is for the next relationship, not this one.
    pub mediator: String,
    /// What the invitation called the counterparty, when it said.
    pub label: Option<String>,
}

/// A message of a relationship, kept: one that came through it and was
/// opened, or one this wallet sent through it — the receipt the spec has
/// the wallet write for itself, so that what somebody did is on record
/// without anything travelling back to say so.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The message's own id, which is what confirms it to the mediator and
    /// what keeps a redelivery from being kept twice.
    pub id: String,
    /// The relationship it belongs to, by counterparty.
    pub counterparty: String,
    /// The protocol message type, a URI.
    #[serde(rename = "type")]
    pub type_: String,
    /// Who the envelope said it was from, when it was authenticated — or,
    /// for what this wallet sent, the pairwise it sent as.
    pub from: Option<String>,
    pub body: Value,
    /// The time the sender wrote into the message, in seconds since the
    /// epoch, when it wrote one.
    pub created_time: Option<u64>,
    /// Whether somebody has opened it in the wallet. What the wallet sent
    /// was read as it was written.
    pub read: bool,
    /// Whether this wallet is the one that sent it. Absent from a book
    /// written before the wallet sent anything, which reads as received.
    #[serde(default)]
    pub sent: bool,
    /// The thread the message names, and the thread above it — for a
    /// credential request, the run the marketplace's invitation began. The
    /// inbox reads the book by these: a request and everything said about
    /// it is one thing to the person, however many messages it took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_thread: Option<String>,
    /// For a credential request this wallet sent, what the person saw and
    /// authorised: the credential by name and the answers under their
    /// labels, which the body does not carry. Absent from anything else,
    /// and from a request written before the wallet kept it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
}

/// Everything in the file.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub relationships: Vec<Relationship>,
    pub messages: Vec<Entry>,
    /// The invitation on show, if one is: the salt its key is walked to,
    /// and the mailbox it is collected from. Absent from a book written
    /// before there were invitations, which reads as none on show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite: Option<Invite>,
}

impl Book {
    /// The relationship with `counterparty`, if there is one.
    pub fn relationship(&self, counterparty: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|r| r.counterparty == counterparty)
    }

    /// Keeps a message, unless one with that id is already here.
    pub fn keep(&mut self, message: Entry) -> bool {
        if self.messages.iter().any(|m| m.id == message.id) {
            return false;
        }
        self.messages.push(message);
        true
    }
}

/// The sealed form on disk.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sealed {
    version: u32,
    nonce: String,
    ciphertext: String,
}

/// The book, opened with the seed. Nothing on disk is an empty book.
pub fn read<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
) -> Result<Book, MessagingError> {
    let bytes = match fs::read(file(app)?) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Book::default()),
        Err(_) => return Err(MessagingError::Storage),
    };
    let sealed: Sealed = serde_json::from_slice(&bytes).map_err(|_| MessagingError::Unreadable)?;
    if sealed.version > VERSION {
        return Err(MessagingError::TooNew);
    }
    let nonce = B64
        .decode(&sealed.nonce)
        .map_err(|_| MessagingError::Unreadable)?;
    let ciphertext = B64
        .decode(&sealed.ciphertext)
        .map_err(|_| MessagingError::Unreadable)?;
    let key = book_key(seed);
    let plaintext = XChaCha20Poly1305::new(Key::from_slice(key.as_slice()))
        .decrypt(
            XNonce::from_slice(nonce.get(..NONCE_BYTES).ok_or(MessagingError::Unreadable)?),
            Payload {
                msg: &ciphertext,
                aad: AAD,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| MessagingError::Unreadable)?;
    serde_json::from_slice(&plaintext).map_err(|_| MessagingError::Unreadable)
}

/// Seals the book and writes it, beside itself and then into place, so a
/// wallet interrupted halfway is left with the book it had.
pub fn write<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    book: &Book,
) -> Result<(), MessagingError> {
    let plaintext = Zeroizing::new(serde_json::to_vec(book).map_err(|_| MessagingError::Storage)?);
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::getrandom(&mut nonce).map_err(|_| MessagingError::Entropy)?;
    let key = book_key(seed);
    let ciphertext = XChaCha20Poly1305::new(Key::from_slice(key.as_slice()))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: AAD,
            },
        )
        .map_err(|_| MessagingError::Storage)?;
    let sealed = Sealed {
        version: VERSION,
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    };
    let bytes = serde_json::to_vec(&sealed).map_err(|_| MessagingError::Storage)?;

    let path = file(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| MessagingError::Storage)?;
    }
    let temporary = path.with_extension("json.writing");
    let mut handle = fs::File::create(&temporary).map_err(|_| MessagingError::Storage)?;
    handle
        .write_all(&bytes)
        .map_err(|_| MessagingError::Storage)?;
    handle.sync_all().map_err(|_| MessagingError::Storage)?;
    drop(handle);
    fs::rename(&temporary, &path).map_err(|_| MessagingError::Storage)
}

/// Removes the book, for an identity that is leaving the device.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(path) = file(app) {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.writing"));
    }
}

fn book_key(seed: &[u8; 64]) -> Zeroizing<[u8; 32]> {
    Zeroizing::new(keys::walk(seed, &[keys::BOOK]))
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, MessagingError> {
    app.path()
        .app_local_data_dir()
        .map(|dir| dir.join(FILE))
        .map_err(|_| MessagingError::Storage)
}
