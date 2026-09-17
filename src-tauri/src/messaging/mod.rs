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
//! What arrives is opened and kept; what this wallet sends is kept too, as
//! the receipt the spec has the wallet write for itself. A relationship is
//! opened two ways: from an invitation — [`invitation`] — that somebody
//! scanned or pasted, where it is the person who opens it from a screen
//! that says who it is with; or from the code this wallet shows —
//! [`invite`] — which whoever is in front of it scans, and which becomes a
//! relationship when they write to it and the wallet next collects.
//!
//! One protocol is spoken, the platform's own for asking an issuer for a
//! credential — [`request`]. The marketplace's invitation is answered with
//! `accept` as it is opened, which is what moves the page that showed it on
//! to the form; and the second code the page then shows, read here, is
//! sent as `request`, which is the form in the issuer's mailbox. Only the
//! `request` is kept: it is what was asked, with what, and the receipt the
//! spec has the wallet write. The `accept` says nothing but "go on", and a
//! signal to a page is not a message of the conversation.

mod book;
mod did;
mod invitation;
mod invite;
mod mediator;
mod pairwise;
mod request;

use didcomm::Message;
use serde::{Serialize, Serializer};
use serde_json::json;
use tauri::{Runtime, State};

use crate::identity::Held;
pub use book::{Book, Entry, Relationship};
pub use invitation::Invitation;
pub use request::Request;

/// What the wallet says first from a pairwise that took over from an
/// invitation key: a ping that asks for nothing, there to carry `from_prior`.
const TRUST_PING: &str = "https://didcomm.org/trust-ping/2.0/ping";

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
    /// What was scanned is not the code the portal shows for a request.
    RequestUnreadable,
    /// The counterparty could not be written to: its document did not
    /// resolve, named no mailbox, or the mailbox did not take the message.
    CounterpartyUnreachable,
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
            Self::RequestUnreadable => "messaging_request_unreadable",
            Self::CounterpartyUnreachable => "messaging_counterparty_unreachable",
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

/// The invitation on show, as the screen draws it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    /// The out-of-band invitation as a URL, which is what the code carries.
    pub url: String,
}

/// Who an invitation is from, for the screen that asks whether to open it.
#[tauri::command]
pub fn messaging_read_invitation(input: String) -> Result<Invitation, MessagingError> {
    invitation::read(&input)
}

/// A fresh invitation for whoever is in front of the wallet: a new key, its
/// mailbox opened at `mediator`, written down in place of whatever was on
/// show before. The previous invitation is over from this moment, answered
/// or not, and its mailbox is given back to the mediator — as far as the
/// mediator can be reached for it; one that cannot is left to its lease.
#[tauri::command]
pub async fn messaging_invite<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    mediator: String,
) -> Result<Shown, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    if !(mediator.starts_with("did:web:") || mediator.starts_with("did:key:")) {
        return Err(MessagingError::MediatorUnreachable);
    }
    let resolver = did::Resolver::new();
    let mut book = book::read(&app, &seed)?;
    if let Some(previous) = book.invite.take() {
        retire(&resolver, &seed, &previous).await;
        // Written before the new one is asked for: a wallet interrupted
        // here has no invitation on show, which is the truth of it.
        book::write(&app, &seed, &book)?;
    }

    let invite = invite::Invite::draw(&seed, &mediator)?;
    let key = invite.key(&seed)?;
    mediator::Mailbox::new(&resolver, &key, &mediator)
        .open()
        .await?;
    let url = invite.url();
    book.invite = Some(invite);
    book::write(&app, &seed, &book)?;
    Ok(Shown { url })
}

/// Gives an invitation's mailbox back. Best effort: an invitation whose key
/// cannot be walked to again, or whose mediator does not answer, is one the
/// mediator's lease will take eventually, and nothing here depends on it.
async fn retire(resolver: &did::Resolver, seed: &[u8; 64], invite: &invite::Invite) {
    let Ok(key) = invite.key(seed) else {
        return;
    };
    if mediator::Mailbox::new(resolver, &key, &invite.mediator)
        .close()
        .await
        .is_err()
    {
        log::info!(
            target: crate::develop::TARGET,
            "an invitation's mailbox could not be given back"
        );
    }
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
///
/// The marketplace's invitation is answered as well as opened: the person
/// pressing the button is the spec's "authorises the start of the request",
/// and `accept` — from the pairwise, naming the invitation as its parent
/// thread — is what tells the page that showed the code to go on to the
/// form. Sent and not kept: nothing has been asked yet, and the thread
/// begins with the request, once the form comes back as the second code.
#[tauri::command]
pub async fn messaging_open<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    invitation: String,
    mediator: String,
) -> Result<Relationship, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let read = invitation::read(&invitation)?;
    let resolver = did::Resolver::new();
    let mut book = book::read(&app, &seed)?;
    let (relationship, pairwise) = ensure(
        &app,
        &seed,
        &resolver,
        &mut book,
        &read.counterparty,
        read.label.clone(),
        &mediator,
    )
    .await?;

    if read.asks_for_credential() {
        let acquisition = read.id.clone().unwrap_or_default();
        let message = Message::build(
            uuid::Uuid::new_v4().to_string(),
            request::ACCEPT_TYPE.to_owned(),
            json!({}),
        )
        .pthid(acquisition)
        .from(pairwise.did.clone())
        .to(read.counterparty.clone())
        .created_time(now())
        .finalize();
        mediator::send(&resolver, &pairwise, &read.counterparty, message)
            .await
            .map_err(|_| MessagingError::CounterpartyUnreachable)?;
    }
    Ok(relationship)
}

/// The relationship with `counterparty`, opened if it was not: the pairwise
/// derived, its mailbox opened at `mediator`, and the relationship written
/// down. One that was there already keeps its mediator and its label.
async fn ensure<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    resolver: &did::Resolver,
    book: &mut Book,
    counterparty: &str,
    label: Option<String>,
    mediator: &str,
) -> Result<(Relationship, pairwise::Pairwise), MessagingError> {
    let pairwise = pairwise::Pairwise::derive(seed, counterparty);
    if let Some(existing) = book.relationship(counterparty) {
        let existing = existing.clone();
        mediator::Mailbox::new(resolver, &pairwise, &existing.mediator)
            .open()
            .await?;
        return Ok((existing, pairwise));
    }
    if !(mediator.starts_with("did:web:") || mediator.starts_with("did:key:")) {
        return Err(MessagingError::MediatorUnreachable);
    }
    mediator::Mailbox::new(resolver, &pairwise, mediator)
        .open()
        .await?;
    let relationship = Relationship {
        counterparty: counterparty.to_owned(),
        pairwise: pairwise.did.clone(),
        mediator: mediator.to_owned(),
        label,
    };
    book.relationships.push(relationship.clone());
    book::write(app, seed, book)?;
    Ok((relationship, pairwise))
}

/// Sends `message` to `counterparty` as `pairwise`, and keeps it as sent,
/// with `summary` — what the person authorised — beside it. Kept only once
/// the counterparty's mediator has taken it: a message that never left is
/// not something that happened.
async fn send_and_keep<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    resolver: &did::Resolver,
    book: &mut Book,
    pairwise: &pairwise::Pairwise,
    counterparty: &str,
    message: Message,
    summary: Option<request::Summary>,
) -> Result<(), MessagingError> {
    mediator::send(resolver, pairwise, counterparty, message.clone())
        .await
        .map_err(|_| MessagingError::CounterpartyUnreachable)?;
    book.keep(Entry {
        id: message.id,
        counterparty: counterparty.to_owned(),
        type_: message.type_,
        from: Some(pairwise.did.clone()),
        body: message.body,
        created_time: message.created_time,
        read: true,
        sent: true,
        thread: message.thid,
        parent_thread: message.pthid,
        summary,
    });
    book::write(app, seed, book)
}

/// What sending came to: whom it went to, and the thread it is now part of,
/// which is where the screen goes next.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    pub counterparty: String,
    pub thread: String,
}

/// Seconds since the epoch, for the time a message says it was written.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The second code, read: what the request would be, for the screen that
/// shows it before anybody sends it.
#[tauri::command]
pub fn messaging_read_request(input: String) -> Result<Request, MessagingError> {
    request::read(&input)
}

/// Sends the request the second code describes to the issuer it names, as
/// the pairwise for that issuer — opening the relationship first if this
/// wallet has none, which is a code scanned by a wallet other than the one
/// that accepted. Kept as sent: the spec's receipt, written locally.
#[tauri::command]
pub async fn messaging_send_request<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    input: String,
    mediator: String,
) -> Result<Sent, MessagingError> {
    let seed = held.seed().ok_or(MessagingError::Locked)?;
    let read = request::read(&input)?;
    let resolver = did::Resolver::new();
    let mut book = book::read(&app, &seed)?;
    let (relationship, pairwise) = ensure(
        &app,
        &seed,
        &resolver,
        &mut book,
        &read.issuer,
        Some(read.issuer_name.clone()),
        &mediator,
    )
    .await?;

    let id = uuid::Uuid::new_v4().to_string();
    let message = Message::build(id.clone(), request::REQUEST_TYPE.to_owned(), read.body())
        .thid(id)
        .pthid(read.acquisition.clone())
        .from(pairwise.did.clone())
        .to(read.issuer.clone())
        .created_time(now())
        .finalize();
    send_and_keep(
        &app,
        &seed,
        &resolver,
        &mut book,
        &pairwise,
        &read.issuer,
        message,
        Some(read.summary()),
    )
    .await?;
    Ok(Sent {
        counterparty: relationship.counterparty,
        thread: read.acquisition,
    })
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

    // The invitation on show first: what answers it becomes a relationship,
    // and a relationship is what the rest of this collects for.
    if let Some(invite) = book.invite.clone() {
        match answered(&app, &seed, &resolver, &mut book, &invite).await {
            Ok(answer) => {
                received += answer.received;
                unreachable.extend(answer.unreachable);
            }
            Err(_) => unreachable.push(invite.did),
        }
    }

    for relationship in book.relationships.clone() {
        let pairwise = pairwise::Pairwise::derive(&seed, &relationship.counterparty);
        let mailbox = mediator::Mailbox::new(&resolver, &pairwise, &relationship.mediator);
        let delivered = match mailbox.take().await {
            Ok(delivered) => delivered,
            // Refused is what a mediator says to a DID it has no mailbox
            // for — one its lease took, or one it lost. The relationship is
            // still this wallet's, so the mailbox is asked for again, once,
            // and what arrives from now on has somewhere to go.
            Err(MessagingError::MediatorRefused) => {
                let again = match mailbox.open().await {
                    Ok(()) => mailbox.take().await,
                    Err(error) => Err(error),
                };
                match again {
                    Ok(delivered) => delivered,
                    Err(_) => {
                        unreachable.push(relationship.counterparty.clone());
                        continue;
                    }
                }
            }
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
            let kept = book.keep(Entry {
                id: message.id,
                counterparty: relationship.counterparty.clone(),
                type_: message.type_,
                from: message.from,
                body: message.body,
                created_time: message.created_time,
                read: false,
                sent: false,
                thread: message.thid,
                parent_thread: message.pthid,
                summary: None,
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

/// Empties the invitation's mailbox, and opens a relationship with whoever
/// wrote to it.
///
/// **The envelope is what says who.** Only a message the invitation key
/// opened as authenticated — authcrypt, from a DID the wallet reaches — names
/// a counterparty; anything else is taken off the mediator and dropped. For
/// each counterparty: the pairwise is derived, which is the same pairwise if
/// the relationship already existed; its mailbox is opened; the relationship
/// is written down if it was not; the message is kept under it; and the
/// wallet answers from the pairwise, carrying `from_prior` signed by the
/// invitation key, so the other side moves the relationship to the pairwise.
/// A relationship that was there already keeps its mediator; a new one takes
/// the invitation's.
///
/// An invitation somebody answered is spent, whether or not the answer got
/// back to them: a mailbox that received once is not shown again, and it is
/// given back to the mediator once what it held is confirmed. The book is
/// written before the mediator is told, so an interruption in between is
/// delivered again rather than lost — and refused by the book as a duplicate.
async fn answered<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    resolver: &did::Resolver,
    book: &mut Book,
    invite: &invite::Invite,
) -> Result<Collected, MessagingError> {
    let key = invite.key(seed)?;
    let mailbox = mediator::Mailbox::new(resolver, &key, &invite.mediator);
    let delivered = mailbox.take().await?;
    if delivered.is_empty() {
        return Ok(Collected {
            received: 0,
            unreachable: Vec::new(),
        });
    }

    let ids: Vec<String> = delivered.iter().map(|d| d.id.clone()).collect();
    let mut received = 0;
    let mut unreachable = Vec::new();
    let mut spent = false;

    for mediator::Delivered {
        message,
        authenticated,
        ..
    } in delivered
    {
        let counterparty = match message.from.as_deref() {
            Some(from)
                if authenticated
                    && (from.starts_with("did:web:") || from.starts_with("did:key:")) =>
            {
                from.to_owned()
            }
            _ => {
                log::info!(
                    target: crate::develop::TARGET,
                    "an answer to the invitation did not say who from"
                );
                continue;
            }
        };
        spent = true;

        let pairwise = pairwise::Pairwise::derive(seed, &counterparty);
        let relationship = match book.relationship(&counterparty) {
            Some(existing) => existing.clone(),
            None => {
                let opened = Relationship {
                    counterparty: counterparty.clone(),
                    pairwise: pairwise.did.clone(),
                    mediator: invite.mediator.clone(),
                    label: None,
                };
                book.relationships.push(opened.clone());
                opened
            }
        };
        if mediator::Mailbox::new(resolver, &pairwise, &relationship.mediator)
            .open()
            .await
            .is_err()
        {
            unreachable.push(counterparty.clone());
        }
        if book.keep(Entry {
            id: message.id,
            counterparty: counterparty.clone(),
            type_: message.type_,
            from: message.from,
            body: message.body,
            created_time: message.created_time,
            read: false,
            sent: false,
            thread: message.thid,
            parent_thread: message.pthid,
            summary: None,
        }) {
            received += 1;
        }

        if rotate(resolver, &key, invite, &pairwise, &counterparty)
            .await
            .is_err()
        {
            log::info!(
                target: crate::develop::TARGET,
                "the answer from the pairwise did not reach the counterparty"
            );
            if !unreachable.contains(&counterparty) {
                unreachable.push(counterparty);
            }
        }
    }

    if spent {
        book.invite = None;
    }
    book::write(app, seed, book)?;
    if mailbox.confirm(&ids).await.is_err() {
        unreachable.push(invite.did.clone());
    } else if spent {
        retire(resolver, seed, invite).await;
    }
    Ok(Collected {
        received,
        unreachable,
    })
}

/// Tells `counterparty` that the invitation key is now the pairwise: a ping
/// from the pairwise, asking for nothing, with `from_prior` — the claim
/// "`iss` is now `sub`", signed by the invitation key, which is the one the
/// counterparty wrote to and so the one whose word counts.
async fn rotate(
    resolver: &did::Resolver,
    key: &pairwise::Pairwise,
    invite: &invite::Invite,
    pairwise: &pairwise::Pairwise,
    counterparty: &str,
) -> Result<(), MessagingError> {
    let (from_prior, _) = didcomm::FromPrior::build(invite.did.clone(), pairwise.did.clone())
        .finalize()
        .pack(
            Some(&format!("{}#key-1", invite.did)),
            resolver,
            &key.secrets(),
        )
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?;
    let ping = Message::build(
        uuid::Uuid::new_v4().to_string(),
        TRUST_PING.to_owned(),
        json!({ "response_requested": false }),
    )
    .from(pairwise.did.clone())
    .to(counterparty.to_owned())
    .from_prior(from_prior)
    .created_time(now())
    .finalize();
    mediator::send(resolver, pairwise, counterparty, ping).await
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
