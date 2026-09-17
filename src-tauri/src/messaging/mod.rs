//! Messaging: the relationships this wallet has, and what comes through them.
//!
//! The spec's design, as this side holds it up:
//!
//! - **One pairwise per relationship**, derived from the seed and from who the
//!   relationship is with — [`pairwise`]. The identity itself is never what
//!   speaks to anybody.
//! - **One mailbox per pairwise**, at the mediator that was chosen when the
//!   relationship was opened — [`mediator`]. No mailbox receives for another,
//!   and no request names two pairwise at once, so the mediator is never
//!   handed the fact that two of them are one person.
//! - **The association is the wallet's alone**: which pairwise is whose lives
//!   in the sealed [`book`], readable only while the wallet is open.
//!
//! What arrives is opened and kept; what somebody does about it is the
//! protocol's business and the next thing to build. A relationship is opened
//! from an invitation — [`invitation`] — that somebody scanned or pasted, and
//! it is the person who opens it, from a screen that says who it is with.

mod book;
mod did;
mod invitation;
mod mediator;
mod pairwise;

use serde::{Serialize, Serializer};
use tauri::{Runtime, State};

use crate::identity::Held;
pub use book::{Book, Received, Relationship};
pub use invitation::Invitation;

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum MessagingError {
    /// No identity is open, so there is no pairwise to speak as and no key to
    /// read the book with.
    Locked,
    /// What was scanned or pasted is not an invitation this wallet reads.
    InvitationUnreadable,
    /// The mediator's document could not be read, or it did not answer.
    MediatorUnreachable,
    /// The mediator answered, and the answer was no.
    MediatorRefused,
    /// The book on disk is not one this wallet reads.
    Unreadable,
    /// A book a later version of the wallet wrote.
    TooNew,
    /// The book could not be read from or written to disk.
    Storage,
    /// The system would not supply randomness.
    Entropy,
}

impl MessagingError {
    const fn code(self) -> &'static str {
        match self {
            Self::Locked => "messaging_locked",
            Self::InvitationUnreadable => "messaging_invitation_unreadable",
            Self::MediatorUnreachable => "messaging_mediator_unreachable",
            Self::MediatorRefused => "messaging_mediator_refused",
            Self::Unreadable => "messaging_unreadable",
            Self::TooNew => "messaging_too_new",
            Self::Storage => "messaging_storage",
            Self::Entropy => "messaging_entropy",
        }
    }
}

impl Serialize for MessagingError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// What a collection came to.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collected {
    /// Messages kept that were not here before.
    pub received: usize,
    /// Relationships whose mediator did not answer, or answered no. Named by
    /// counterparty, so the screen can say which.
    pub unreachable: Vec<String>,
}

/// Who an invitation is from, for the screen that asks whether to open it.
#[tauri::command]
pub fn messaging_read_invitation(input: String) -> Result<Invitation, MessagingError> {
    invitation::read(&input)
}

/// Everything the book holds.
#[tauri::command(async)]
pub fn messaging_book<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Book, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    book::read(&app, &seed)
}

/// Opens a relationship from an invitation: derives the pairwise for whoever
/// it is from, opens that pairwise's mailbox at `mediator`, and writes the
/// relationship down. Opening one that already exists opens the mailbox again
/// — which the mediator answers the same — and changes nothing in the book.
#[tauri::command]
pub async fn messaging_open<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    invitation: String,
    mediator: String,
) -> Result<Relationship, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let read = invitation::read(&invitation)?;
    if !(mediator.starts_with("did:web:") || mediator.starts_with("did:key:")) {
        return Err(MessagingError::MediatorUnreachable);
    }

    let pairwise = pairwise::Pairwise::derive(&seed, &read.counterparty);
    let resolver = did::Resolver::new();
    mediator::Mailbox::new(&resolver, &pairwise, &mediator)
        .open()
        .await?;

    let mut book = book::read(&app, &seed)?;
    if let Some(existing) = book.relationship(&read.counterparty) {
        return Ok(existing.clone());
    }
    let relationship = Relationship {
        counterparty: read.counterparty,
        pairwise: pairwise.did,
        mediator,
        label: read.label,
    };
    book.relationships.push(relationship.clone());
    book::write(&app, &seed, &book)?;
    Ok(relationship)
}

/// Empties every mailbox, one relationship at a time, and keeps what came.
///
/// A mediator that does not answer for one relationship does not stop the
/// others: what it comes to is reported, by counterparty, and the rest is
/// collected. Confirmed to the mediator only once written down here, so a
/// wallet interrupted in between is delivered again rather than never.
#[tauri::command]
pub async fn messaging_collect<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Collected, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let mut book = book::read(&app, &seed)?;
    let resolver = did::Resolver::new();
    let mut received = 0;
    let mut unreachable = Vec::new();

    for relationship in book.relationships.clone() {
        let pairwise = pairwise::Pairwise::derive(&seed, &relationship.counterparty);
        let mailbox = mediator::Mailbox::new(&resolver, &pairwise, &relationship.mediator);
        let delivered = match mailbox.take().await {
            Ok(delivered) => delivered,
            Err(_) => {
                unreachable.push(relationship.counterparty.clone());
                continue;
            }
        };
        if delivered.is_empty() {
            continue;
        }
        let ids: Vec<String> = delivered.iter().map(|d| d.id.clone()).collect();
        for mediator::Delivered { message, .. } in delivered {
            let kept = book.keep(Received {
                id: message.id,
                counterparty: relationship.counterparty.clone(),
                type_: message.type_,
                from: message.from,
                body: message.body,
                created_time: message.created_time,
                read: false,
            });
            if kept {
                received += 1;
            }
        }
        book::write(&app, &seed, &book)?;
        if mailbox.confirm(&ids).await.is_err() {
            // Kept here already; the mediator will offer them again and the
            // book will refuse the duplicates.
            unreachable.push(relationship.counterparty.clone());
        }
    }

    Ok(Collected {
        received,
        unreachable,
    })
}

/// Marks a message as opened.
#[tauri::command(async)]
pub fn messaging_mark_read<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    id: String,
) -> Result<(), MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let mut book = book::read(&app, &seed)?;
    let Some(message) = book.messages.iter_mut().find(|m| m.id == id) else {
        return Ok(());
    };
    if message.read {
        return Ok(());
    }
    message.read = true;
    book::write(&app, &seed, &book)
}

/// Removes the book, for an identity that is leaving the device. The
/// relationships go with it: what they were is nobody's to keep once the
/// seed that named them is gone.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    book::clear(app);
}
