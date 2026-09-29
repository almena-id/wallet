//! Messaging: the wallet's mediation with a DIDComm mediator, and the
//! relationships it has through it.
//!
//! A wallet has no address of its own, so it gets one from a mediator: it asks
//! for mediation (Coordinate Mediation 3.0), registers the DIDs it can be
//! reached at, and from then on anybody can reach one of them by wrapping a
//! message in a `forward` to the mediator, which queues it until the wallet
//! picks it up (Message Pickup 3.0). The protocols, and what the mediator
//! requires of them, are in the mediator's `docs/didcomm.md` §4–5.
//!
//! - [`peer`]: the DIDs the wallet speaks as — inbox, contact card, pairwise —
//!   all derived from the seed.
//! - [`mediator`]: finding a mediator and talking to it.
//! - [`contacts`]: invitations and the handshake that opens a relationship.
//! - [`chat`]: what is said once it is open — text, and each side's name.
//! - [`state`]: the mediator, this wallet's name and the relationships, sealed
//!   under the seed.
//! - [`conversation`]: what was said in each relationship, sealed the same way.
//! - [`live`]: a WebSocket to the mediator while the wallet is in front, down
//!   which messages arrive as they are sent.
//! - [`photo`]: the picture the wallet shows for its own identity.
//! - [`push`]: the device token the mediator notifies while the wallet is not
//!   running.
//! - [`call`]: the signalling of calls, and the TURN credentials they are
//!   relayed with (`SPEC.md` §3).
//!
//! Everything here needs the wallet open: every key and the state key come
//! from the seed, which is only held while it is.

mod call;
mod chat;
mod contacts;
mod conversation;
pub mod live;
pub(crate) mod mediator;
mod peer;
mod photo;
mod push;
mod state;

use almena_didcomm::{unpack, InMemorySecrets, Message};
use serde::{Serialize, Serializer};
use serde_json::json;
use tauri::async_runtime::Mutex;
use tauri::{Emitter, Manager, Runtime, State};
use zeroize::Zeroizing;

use crate::identity::Held;
use conversation::Entry;
use mediator::Mediator;
use peer::Peer;
use state::{Last, Mediation, Relationship};

const STATUS_REQUEST: &str = "https://didcomm.org/messagepickup/3.0/status-request";

/// How many `delivery-request`s one sync makes at most.
const SYNC_ROUNDS: usize = 10;

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum MessagingError {
    /// No identity is open, so there are no keys to speak with.
    Locked,
    /// What was pasted is not an invitation, address or DID this wallet reads.
    InvitationUnreadable,
    /// This wallet's own invitation, pasted back into it.
    InvitationOwn,
    /// A mediator that would be reached over plain HTTP.
    Insecure,
    /// The mediator did not answer, or its answer could not be read.
    MediatorUnreachable,
    /// The mediator answered, and the answer was no.
    MediatorRefused,
    /// The counterparty could not be written to: its DID did not resolve,
    /// named no mailbox, or the mailbox did not take the message.
    CounterpartyUnreachable,
    /// Asked for something that needs a mediation this wallet does not have.
    NotConnected,
    /// Written to somebody who has not answered the invitation yet.
    Pending,
    /// A contact or a message this wallet does not have.
    ContactUnknown,
    /// A message that is empty or too long to send.
    MessageInvalid,
    /// A picture that is not a small JPEG.
    PhotoInvalid,
    /// The keys could not be made or used.
    Keys,
    /// The state on disk is not one this wallet can open.
    Unreadable,
    /// The state could not be read from or written to disk.
    Storage,
    /// The system would not supply randomness.
    Entropy,
    /// The mediator offers no TURN relay, so no call can be placed.
    CallsUnavailable,
}

impl MessagingError {
    const fn code(self) -> &'static str {
        match self {
            Self::Locked => "messaging_locked",
            Self::InvitationUnreadable => "messaging_invitation_unreadable",
            Self::InvitationOwn => "messaging_invitation_own",
            Self::Insecure => "messaging_insecure",
            Self::MediatorUnreachable => "messaging_mediator_unreachable",
            Self::MediatorRefused => "messaging_mediator_refused",
            Self::CounterpartyUnreachable => "messaging_counterparty_unreachable",
            Self::NotConnected => "messaging_not_connected",
            Self::Pending => "messaging_pending",
            Self::ContactUnknown => "messaging_contact_unknown",
            Self::MessageInvalid => "messaging_message_invalid",
            Self::PhotoInvalid => "messaging_photo_invalid",
            Self::Keys => "messaging_keys",
            Self::Unreadable => "messaging_unreadable",
            Self::Storage => "messaging_storage",
            Self::Entropy => "messaging_entropy",
            Self::CallsUnavailable => "messaging_calls_unavailable",
        }
    }
}

impl Serialize for MessagingError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// Serialises everything that reads the state and writes it back, so a sync
/// and an accepted invitation cannot both start from the same copy. Held
/// across the network calls in between, which is why it is an async lock.
#[derive(Default)]
pub struct Gate(Mutex<()>);

/// The mediation as the interface shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediatorStatus {
    /// The mediator's DID, when there is a mediation.
    pub mediator: Option<String>,
    /// The inbox DID registered with it.
    pub inbox: Option<String>,
}

impl From<Option<&Mediation>> for MediatorStatus {
    fn from(mediation: Option<&Mediation>) -> Self {
        Self {
            mediator: mediation.map(|m| m.mediator.clone()),
            inbox: mediation.map(|m| m.inbox.clone()),
        }
    }
}

/// What is waiting at the mediator.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Waiting {
    pub message_count: u64,
}

/// A relationship as the interface shows it. The DIDs stay on this side.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    /// The conversation's id, which the commands that act on one take.
    pub id: String,
    /// What it is called: the alias, else the name they gave, else the
    /// fingerprint.
    pub name: String,
    /// The name this wallet gave it.
    pub alias: Option<String>,
    /// The name they gave themselves.
    pub their_name: Option<String>,
    /// A short fingerprint of the DID the relationship was opened with.
    pub fingerprint: String,
    /// Waiting for the counterparty's first answer.
    pub pending: bool,
    pub since: u64,
    pub unread: u32,
    pub last: Option<Last>,
    /// Its history was deleted; the inbox leaves it out until it has news.
    pub cleared: bool,
}

impl From<&Relationship> for Contact {
    fn from(relationship: &Relationship) -> Self {
        let fingerprint = contacts::name(&relationship.origin);
        Self {
            id: conversation::id(&relationship.ours),
            name: relationship
                .alias
                .clone()
                .or_else(|| relationship.name.clone())
                .unwrap_or_else(|| fingerprint.clone()),
            alias: relationship.alias.clone(),
            their_name: relationship.name.clone(),
            fingerprint,
            pending: relationship.pending,
            since: relationship.since,
            unread: relationship.unread,
            last: relationship.last.clone(),
            cleared: relationship.cleared,
        }
    }
}

/// A conversation as the interface shows it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub contact: Contact,
    /// Oldest first.
    pub entries: Vec<Entry>,
}

/// The name this wallet goes by.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: Option<String>,
    /// Contacts a changed name could not be sent to.
    pub unreached: usize,
}

/// The wallet's own invitation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    pub url: String,
}

/// What a sync came to.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Synced {
    /// Relationships opened, confirmed or renamed, and messages, by what
    /// arrived.
    pub changed: usize,
    pub contacts: Vec<Contact>,
}

/// The mediation this wallet has, if any. Reads the device only.
#[tauri::command]
pub fn mediator_status<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<MediatorStatus, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;
    Ok(state.mediation.as_ref().into())
}

/// Connects to the mediator somebody pasted: asks for mediation, registers the
/// inbox with it, and writes down that it did.
///
/// Asking again is harmless: the mediator grants the same mediation to the same
/// inbox and answers `no_change` to a DID it already has.
#[tauri::command]
pub async fn mediator_connect<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    input: String,
) -> Result<MediatorStatus, MessagingError> {
    let seed = seed(&held)?;
    let mediation = mediate(&seed, &input).await?;

    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    state.mediation = Some(mediation);
    state::write(&app, &seed, &state)?;

    // Live delivery gave up for want of a mediation, or follows the old one:
    // without this nothing arrives live — a call included — until the wallet
    // next comes to the front.
    app.state::<live::Live>().restart(app.clone());

    Ok(state.mediation.as_ref().into())
}

/// Asks the mediator how many messages it is holding for this wallet.
#[tauri::command]
pub async fn mediator_check<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Waiting, MessagingError> {
    let seed = seed(&held)?;
    let mediation = state::read(&app, &seed)?
        .mediation
        .ok_or(MessagingError::NotConnected)?;
    waiting(&seed, &mediation.mediator).await
}

/// Leaves the mediator: takes the inbox off its recipients if it answers, and
/// forgets the mediation either way — a mediator that is gone must not keep a
/// wallet from choosing another.
#[tauri::command]
pub async fn mediator_disconnect<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<MediatorStatus, MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;

    if let Some(mediation) = &state.mediation {
        if let Ok(mediator) = Mediator::resolve(&mediation.mediator).await {
            if let Ok(inbox) = Peer::inbox(&seed, &mediator.did) {
                let _ = push::register(&mediator, &inbox, None).await;
                let _ = mediator.recipient(&inbox, &inbox, "remove").await;
            }
        }
    }
    push::forget(&app);

    state.mediation = None;
    state::write(&app, &seed, &state)?;
    Ok(MediatorStatus::from(None))
}

/// Registers this device's push token with the mediation, asking for
/// permission to notify the first time. Returns whether the mediator will now
/// notify it: not on desktop, not without a mediation, not when permission was
/// refused, and not with a mediator that does not push for this platform.
#[tauri::command]
pub async fn push_register<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<bool, MessagingError> {
    let seed = seed(&held)?;
    let Some(mediation) = state::read(&app, &seed)?.mediation else {
        return Ok(false);
    };
    let Some(token) = push::token(&app).await else {
        return Ok(false);
    };
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    let inbox = Peer::inbox(&seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;
    match push::register(&mediator, &inbox, Some(&token)).await {
        Ok(()) => Ok(true),
        Err(MessagingError::MediatorRefused) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Takes this device off the mediation's pushes, as far as the mediator can be
/// reached, and tells the system no more are wanted. For signing out: an
/// identity that has left the device must not keep waking it.
#[tauri::command]
pub async fn push_unregister<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<(), MessagingError> {
    let seed = seed(&held)?;
    if let Some(mediation) = state::read(&app, &seed)?.mediation {
        let mediator = Mediator::resolve(&mediation.mediator).await?;
        let inbox = Peer::inbox(&seed, &mediator.did)?;
        let _ = push::register(&mediator, &inbox, None).await;
    }
    push::forget(&app);
    Ok(())
}

/// The relationships this wallet has, the latest activity first. Reads the
/// device only.
#[tauri::command]
pub fn contacts_list<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Vec<Contact>, MessagingError> {
    let seed = seed(&held)?;
    Ok(listed(&state::read(&app, &seed)?.relationships))
}

/// The wallet's own invitation, with its contact card registered at the
/// mediator so that whoever accepts it can reach it.
#[tauri::command]
pub async fn invitation_show<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Shown, MessagingError> {
    let seed = seed(&held)?;
    let mediation = state::read(&app, &seed)?
        .mediation
        .ok_or(MessagingError::NotConnected)?;
    Ok(Shown {
        url: invitation(&seed, &mediation.mediator).await?,
    })
}

/// What a link or a scanned code is, as far as this wallet can tell without
/// acting on it: somebody's invitation, a mediator's, a registry's request, or
/// none of them. Nothing is opened or sent; the screen it leads to asks first.
#[tauri::command]
pub fn invitation_kind(input: String) -> &'static str {
    if crate::registry::is_request(&input) {
        "registry"
    } else if contacts::read(&input).is_ok() {
        "contact"
    } else if mediator::target(&input).is_ok_and(|target| target.invitation.is_some()) {
        "mediator"
    } else {
        "unknown"
    }
}

/// What the confirmation sheet shows about a link or a scanned code, read
/// without acting on it — only what the wallet actually knows, since an
/// invitation carries no name.
#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LinkDetails {
    /// Somebody's invitation: the fingerprint of the card it names, and what
    /// this wallet already has with it.
    Contact {
        fingerprint: String,
        /// The relationship already opened with that card, if there is one.
        contact: Option<Contact>,
        /// This wallet's own invitation.
        own: bool,
    },
    /// A mediator's invitation, beside the mediation this wallet has.
    Mediator {
        mediator: String,
        current: Option<String>,
        /// Relationships whose DIDs route through the current mediator: a new
        /// one would stop collecting what they send.
        contacts: usize,
    },
    Unknown,
}

/// What a link or a scanned code is, with what this wallet knows about it.
/// Reads the device only.
#[tauri::command]
pub fn link_details<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    input: String,
) -> Result<LinkDetails, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;

    if let Ok(invitation) = contacts::read(&input) {
        let own = state.mediation.as_ref().is_some_and(|mediation| {
            Peer::card(&seed, &mediation.mediator).is_ok_and(|card| card.did == invitation.from)
        });
        return Ok(LinkDetails::Contact {
            fingerprint: contacts::name(&invitation.from),
            contact: state
                .relationships
                .iter()
                .find(|r| r.origin == invitation.from)
                .map(Contact::from),
            own,
        });
    }
    if let Ok(target) = mediator::target(&input) {
        if target.invitation.is_some() {
            return Ok(LinkDetails::Mediator {
                mediator: target.did,
                current: state.mediation.map(|m| m.mediator),
                contacts: state.relationships.len(),
            });
        }
    }
    Ok(LinkDetails::Unknown)
}

/// Accepts somebody's invitation, and writes down the relationship it opens.
#[tauri::command]
pub async fn contact_accept<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    input: String,
) -> Result<Contact, MessagingError> {
    let seed = seed(&held)?;
    let invitation = contacts::read(&input)?;

    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let relationship = accept(&seed, &mut state, &invitation).await?;
    state::write(&app, &seed, &state)?;

    Ok(Contact::from(&relationship))
}

/// Picks up what is waiting, handles what opens or confirms a relationship, and
/// returns the relationships as they are after it.
#[tauri::command]
pub async fn messages_sync<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<Synced, MessagingError> {
    let seed = seed(&held)?;

    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let outcome = sync(&seed, &mut state).await?;
    // Asked for from the interface, so somebody is looking: nothing to notify.
    let changed = apply(&app, &seed, &mut state, outcome)?.changed;

    Ok(Synced {
        changed,
        contacts: listed(&state.relationships),
    })
}

/// A conversation and what was said in it. Reads the device only.
#[tauri::command]
pub fn conversation_read<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    id: String,
) -> Result<Conversation, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;
    let relationship = find(&state.relationships, &id)?;
    Ok(Conversation {
        contact: Contact::from(relationship),
        entries: conversation::read(&app, &seed, &id)?,
    })
}

/// Marks a conversation as read.
#[tauri::command]
pub async fn conversation_seen<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    id: String,
) -> Result<(), MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let relationship = find_mut(&mut state.relationships, &id)?;
    if relationship.unread > 0 {
        relationship.unread = 0;
        state::write(&app, &seed, &state)?;
    }
    Ok(())
}

/// Deletes the history of the conversation `id`. The relationship stays, and
/// the conversation is back in the inbox with the next message either way.
#[tauri::command]
pub async fn conversation_clear<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    id: String,
) -> Result<(), MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let relationship = find_mut(&mut state.relationships, &id)?;
    relationship.unread = 0;
    relationship.last = None;
    relationship.cleared = true;
    conversation::remove(&app, &id)?;
    state::write(&app, &seed, &state)
}

/// Sends `content` in the conversation `id`. What the other side's mediator
/// did not take is kept, marked failed, to be retried.
#[tauri::command]
pub async fn message_send<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    id: String,
    content: String,
) -> Result<Entry, MessagingError> {
    let seed = seed(&held)?;
    let content = content.trim();
    if content.is_empty() || content.chars().count() > chat::TEXT_CHARS {
        return Err(MessagingError::MessageInvalid);
    }

    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let message = chat::text(content);
    let entry = Entry {
        id: message.id.clone(),
        mine: true,
        content: content.to_owned(),
        at: message
            .created_time
            .unwrap_or_else(almena_didcomm::message::now),
        failed: false,
    };
    written(&app, &seed, &mut state, &id, entry, message).await
}

/// Sends again a message of this wallet's that failed, under the same `id`, so
/// a copy that did arrive after all is recognised as one.
#[tauri::command]
pub async fn message_retry<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    id: String,
    message: String,
) -> Result<Entry, MessagingError> {
    let seed = seed(&held)?;

    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let entry = conversation::read(&app, &seed, &id)?
        .into_iter()
        .find(|e| e.id == message && e.mine)
        .ok_or(MessagingError::ContactUnknown)?;
    let mut again = chat::text(&entry.content);
    again.id = entry.id.clone();
    again.created_time = Some(entry.at);
    written(&app, &seed, &mut state, &id, entry, again).await
}

/// Gives a contact a name of this wallet's own, or takes it away.
#[tauri::command]
pub async fn contact_rename<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    id: String,
    alias: Option<String>,
) -> Result<Contact, MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let relationship = find_mut(&mut state.relationships, &id)?;
    relationship.alias = alias.as_deref().and_then(chat::clean);
    let contact = Contact::from(&*relationship);
    state::write(&app, &seed, &state)?;
    Ok(contact)
}

/// The picture this wallet shows for its own identity, as a `data:` URL, when
/// there is one. Reads the device only.
#[tauri::command]
pub fn profile_photo_read<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Option<String>, MessagingError> {
    let seed = seed(&held)?;
    photo::read(&app, &seed)
}

/// Keeps a new picture — a JPEG `data:` URL the interface made small — or
/// removes it with `None`. It stays on this device.
#[tauri::command]
pub fn profile_photo_write<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    photo: Option<String>,
) -> Result<(), MessagingError> {
    let seed = seed(&held)?;
    photo::write(&app, &seed, photo.as_deref())
}

/// The name this wallet goes by. Reads the device only.
#[tauri::command]
pub fn profile_read<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Profile, MessagingError> {
    let seed = seed(&held)?;
    Ok(Profile {
        name: state::read(&app, &seed)?.profile,
        unreached: 0,
    })
}

/// Changes the name this wallet goes by, and sends it to every contact.
#[tauri::command]
pub async fn profile_write<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    name: Option<String>,
) -> Result<Profile, MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let name = name.as_deref().and_then(chat::clean);
    if name == state.profile {
        return Ok(Profile { name, unreached: 0 });
    }
    state.profile = name.clone();
    state::write(&app, &seed, &state)?;

    let unreached = announce(&seed, &state).await;
    Ok(Profile { name, unreached })
}

/// The TURN servers a call is relayed through, as `RTCIceServer`s, from this
/// wallet's own mediator. Asked for before each call: the credentials expire.
#[tauri::command]
pub async fn call_ice_servers<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<serde_json::Value, MessagingError> {
    let seed = seed(&held)?;
    let mediation = state::read(&app, &seed)?
        .mediation
        .ok_or(MessagingError::NotConnected)?;
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    let inbox = Peer::inbox(&seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;
    call::ice_servers(&mediator, &inbox).await
}

/// Says `signal` to the contact of the conversation `id`, in the call `call`
/// — none for an offer, which starts one. Returns the call's id.
///
/// Reads the state without the [`Gate`]: nothing is written, and a call must
/// not wait for a sync.
#[tauri::command]
pub async fn call_send<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    id: String,
    call: Option<String>,
    signal: call::Signal,
) -> Result<String, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;
    let mediation = state
        .mediation
        .as_ref()
        .ok_or(MessagingError::NotConnected)?;
    let relationship = find(&state.relationships, &id)?;
    if relationship.pending {
        return Err(MessagingError::Pending);
    }
    let message = call::message(call.as_deref(), &signal)?;
    let call = message.thid().to_owned();
    deliver(&seed, mediation, relationship, message).await?;
    Ok(call)
}

/// Registers the lock the commands that change the state hold, and the live
/// session.
pub fn manage<R: Runtime>(app: &tauri::AppHandle<R>) {
    app.manage(Gate::default());
    app.manage(live::Live::default());
}

/// Forgets everything messaging wrote down. Called when the identity leaves the
/// device, by signing out or by running out of PIN attempts.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(live) = app.try_state::<live::Live>() {
        live.stop();
    }
    state::clear(app);
    conversation::clear(app);
    photo::clear(app);
}

/// Asks for mediation from the mediator `input` names and registers the inbox
/// with it: the protocol half of [`mediator_connect`], without the state.
async fn mediate(seed: &[u8; 64], input: &str) -> Result<Mediation, MessagingError> {
    let target = mediator::target(input)?;
    let mediator = Mediator::resolve(&target.did).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;

    mediator.ensure(&inbox, target.invitation).await?;

    Ok(Mediation {
        mediator: mediator.did,
        inbox: inbox.did,
    })
}

/// How many messages the mediator is holding for the inbox.
async fn waiting(seed: &[u8; 64], mediator: &str) -> Result<Waiting, MessagingError> {
    let mediator = Mediator::resolve(mediator).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;

    let status = mediator
        .request(&inbox, Message::new(STATUS_REQUEST, json!({})))
        .await?;
    let message_count = status.body["message_count"]
        .as_u64()
        .filter(|_| status.type_.ends_with("/status"))
        .ok_or(MessagingError::MediatorUnreachable)?;

    Ok(Waiting { message_count })
}

/// Registers the contact card and returns the invitation that names it.
async fn invitation(seed: &[u8; 64], mediator: &str) -> Result<String, MessagingError> {
    let mediator = Mediator::resolve(mediator).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;
    let card = Peer::card(seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;
    mediator.register(&inbox, &card).await?;
    Ok(contacts::link(&card.did))
}

/// Accepts an invitation into `state`: the protocol half of
/// [`contact_accept`].
async fn accept(
    seed: &[u8; 64],
    state: &mut state::State,
    invitation: &contacts::Invitation,
) -> Result<Relationship, MessagingError> {
    let mediation = state
        .mediation
        .as_ref()
        .ok_or(MessagingError::NotConnected)?;
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;
    let card = Peer::card(seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;

    let relationship = contacts::accept(
        seed,
        &mediator,
        &inbox,
        &card,
        invitation,
        &state.relationships,
    )
    .await?;
    upsert(&mut state.relationships, relationship.clone());
    Ok(relationship)
}

/// A message of text that arrived, for the relationship this wallet is `ours`
/// in.
struct Arrival {
    ours: String,
    entry: Entry,
}

/// What a sync came to, before the conversations are written.
#[derive(Default)]
struct Outcome {
    /// Relationships opened, confirmed or renamed.
    changed: usize,
    arrivals: Vec<Arrival>,
    /// Call signals, handed to the interface as they are.
    calls: Vec<call::Incoming>,
}

/// Picks up and handles what is waiting: the protocol half of
/// [`messages_sync`]. Relationships are changed in `state`; the texts that
/// arrived are returned, for the caller to write into their conversations.
///
/// When the mediator stops answering after the first round, what was taken
/// so far is returned and the rest waits for the next sync.
async fn sync(seed: &[u8; 64], state: &mut state::State) -> Result<Outcome, MessagingError> {
    let mediation = state
        .mediation
        .clone()
        .ok_or(MessagingError::NotConnected)?;
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;
    let card = Peer::card(seed, &mediator.did)?;
    mediator.ensure(&inbox, None).await?;

    let mut outcome = Outcome::default();
    for round in 0..SYNC_ROUNDS {
        let deliveries = match mediator.deliveries(&inbox).await {
            Ok(deliveries) => deliveries,
            Err(_) if round > 0 => break,
            Err(error) => return Err(error),
        };
        if deliveries.is_empty() {
            break;
        }

        let received = handle(
            seed,
            state,
            &mediator,
            &inbox,
            &card,
            deliveries,
            &mut outcome,
        )
        .await?;

        // Nothing taken this round means what is left is for a later version;
        // asking again would only bring the same messages back.
        if received.is_empty() {
            break;
        }
        // Not acknowledged is delivered again, and recognised then by its id.
        if mediator.acknowledge(&inbox, &received).await.is_err() {
            break;
        }
    }

    Ok(outcome)
}

/// Handles one batch of `(queue id, envelope)` pairs — from a `delivery`,
/// asked for or pushed live — into `state` and `outcome`, and returns the ids
/// to acknowledge.
///
/// A message this version does not handle is left queued, so a later version
/// can; one that cannot be opened at all is acknowledged, because it never will
/// be.
async fn handle(
    seed: &[u8; 64],
    state: &mut state::State,
    mediator: &Mediator,
    inbox: &Peer,
    card: &Peer,
    deliveries: Vec<(String, String)>,
    outcome: &mut Outcome,
) -> Result<Vec<String>, MessagingError> {
    // Every DID a message can be for: the card and each pairwise, derived
    // again rather than kept.
    let mut secrets = InMemorySecrets::new();
    inbox.add_to(&mut secrets);
    card.add_to(&mut secrets);
    for relationship in &state.relationships {
        Peer::pairwise(seed, &mediator.did, &relationship.origin)?.add_to(&mut secrets);
    }

    let mut received = Vec::new();
    for (id, envelope) in deliveries {
        let Ok((message, metadata)) = unpack(&envelope, mediator.resolver(), &secrets).await else {
            received.push(id);
            continue;
        };
        if !metadata.authenticated {
            received.push(id);
            continue;
        }
        let to = message.to.clone().unwrap_or_default();

        match message.type_.as_str() {
            contacts::PING if to.contains(&card.did) => {
                // A sender that cannot be answered now is left queued and
                // tried again on the next sync, without holding up the rest.
                if let Ok(relationship) = contacts::welcome(
                    seed,
                    mediator,
                    inbox,
                    card,
                    &message,
                    &state.relationships,
                    state.profile.as_deref(),
                )
                .await
                {
                    upsert(&mut state.relationships, relationship);
                    outcome.changed += 1;
                    received.push(id);
                }
            }
            contacts::PING_RESPONSE => {
                if let Some(relationship) = state
                    .relationships
                    .iter_mut()
                    .find(|r| to.contains(&r.ours))
                {
                    // The rotation the invitation's card signed: the
                    // conversation moves to the pairwise that sent this.
                    if let Some(rotation) = &metadata.from_prior {
                        if rotation.iss == relationship.theirs
                            && message.from.as_deref() == Some(rotation.sub.as_str())
                        {
                            relationship.theirs = rotation.sub.clone();
                        }
                    }
                    if relationship.pending
                        && message.from.as_deref() == Some(relationship.theirs.as_str())
                    {
                        relationship.pending = false;
                        outcome.changed += 1;
                    }
                }
                received.push(id);
            }
            call::OFFER | call::ANSWER | call::HANGUP => {
                let Some(relationship) = state.relationships.iter().find(|r| to.contains(&r.ours))
                else {
                    received.push(id);
                    continue;
                };
                if message.from.as_deref() != Some(relationship.theirs.as_str()) {
                    continue;
                }
                if let Some((call, signal)) = call::read(&message, almena_didcomm::message::now()) {
                    outcome.calls.push(call::Incoming {
                        contact: conversation::id(&relationship.ours),
                        call,
                        signal,
                    });
                }
                received.push(id);
            }
            chat::TEXT | chat::PROFILE => {
                let Some(relationship) = state
                    .relationships
                    .iter_mut()
                    .find(|r| to.contains(&r.ours))
                else {
                    received.push(id);
                    continue;
                };
                // Until the rotation in their ping-response is read, their
                // pairwise is not known; a message that overtook it is
                // left for the round after.
                if message.from.as_deref() != Some(relationship.theirs.as_str()) {
                    continue;
                }

                if message.type_ == chat::TEXT {
                    if let Some(content) = chat::read_text(&message) {
                        outcome.arrivals.push(Arrival {
                            ours: relationship.ours.clone(),
                            entry: Entry {
                                id: message.id.clone(),
                                mine: false,
                                content,
                                at: message
                                    .created_time
                                    .unwrap_or_else(almena_didcomm::message::now),
                                failed: false,
                            },
                        });
                    }
                } else {
                    if let Some(name) = chat::read_profile(&message) {
                        if relationship.name != name {
                            relationship.name = name;
                            outcome.changed += 1;
                        }
                    }
                    if chat::wants_ours(&message) {
                        // Not sent back is not worth holding the rest up
                        // for: the next change of name reaches them.
                        if let Ok(pairwise) =
                            Peer::pairwise(seed, &mediator.did, &relationship.origin)
                        {
                            let _ = chat::send_profile(
                                mediator,
                                &pairwise,
                                &relationship.theirs,
                                state.profile.as_deref(),
                                false,
                            )
                            .await;
                        }
                    }
                }
                received.push(id);
            }
            _ => {}
        }
    }

    Ok(received)
}

/// What [`apply`] came to: how much changed, and the messages that were new —
/// which live delivery may tell the system about (`crate::notify`).
pub(crate) struct Applied {
    pub changed: usize,
    pub fresh: Vec<crate::notify::Fresh>,
}

/// Writes what `outcome` brought into the conversations, counts it into their
/// relationships, and writes the state when anything changed.
///
/// What arrived is already acknowledged, so a conversation that cannot be
/// written to does not stop the rest, nor the state, from being written.
fn apply<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    state: &mut state::State,
    outcome: Outcome,
) -> Result<Applied, MessagingError> {
    let mut changed = outcome.changed;
    let mut fresh = Vec::new();
    for incoming in outcome.calls {
        let _ = app.emit(call::SIGNAL, incoming);
    }
    let mut failure = None;
    for arrival in outcome.arrivals {
        let Some(relationship) = state
            .relationships
            .iter_mut()
            .find(|r| r.ours == arrival.ours)
        else {
            continue;
        };
        let last = Last {
            content: arrival.entry.content.clone(),
            at: arrival.entry.at,
            mine: false,
        };
        match conversation::put(
            app,
            seed,
            &conversation::id(&relationship.ours),
            arrival.entry,
        ) {
            Ok(true) => {
                relationship.unread += 1;
                relationship.cleared = false;
                fresh.push(crate::notify::Fresh {
                    from: Contact::from(&*relationship).name,
                    content: last.content.clone(),
                });
                if relationship.last.as_ref().is_none_or(|l| l.at <= last.at) {
                    relationship.last = Some(last);
                }
                changed += 1;
            }
            Ok(false) => {}
            Err(error) => failure = Some(error),
        }
    }
    if changed > 0 {
        state::write(app, seed, state)?;
    }
    match failure {
        Some(error) => Err(error),
        None => Ok(Applied { changed, fresh }),
    }
}

/// Sends `message`, which `entry` records, in the conversation `id`, and writes
/// down the entry and the conversation's latest message whether it went or
/// not.
async fn written<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    state: &mut state::State,
    id: &str,
    mut entry: Entry,
    message: Message,
) -> Result<Entry, MessagingError> {
    let mediation = state
        .mediation
        .clone()
        .ok_or(MessagingError::NotConnected)?;
    let relationship = find_mut(&mut state.relationships, id)?;
    if relationship.pending {
        return Err(MessagingError::Pending);
    }

    entry.failed = deliver(seed, &mediation, relationship, message)
        .await
        .is_err();
    conversation::put(app, seed, id, entry.clone())?;
    relationship.cleared = false;
    if relationship.last.as_ref().is_none_or(|l| l.at <= entry.at) {
        relationship.last = Some(Last {
            content: entry.content.clone(),
            at: entry.at,
            mine: true,
        });
    }
    state::write(app, seed, state)?;
    Ok(entry)
}

/// Sends `message` to the counterparty of `relationship`, from its pairwise.
async fn deliver(
    seed: &[u8; 64],
    mediation: &Mediation,
    relationship: &Relationship,
    message: Message,
) -> Result<(), MessagingError> {
    let mediator = Mediator::resolve(&mediation.mediator).await?;
    let from = Peer::pairwise(seed, &mediator.did, &relationship.origin)?;
    contacts::send(&mediator, &from, &relationship.theirs, message).await
}

/// Sends this wallet's name to every contact that has answered. Returns how
/// many it could not be sent to.
async fn announce(seed: &[u8; 64], state: &state::State) -> usize {
    let open: Vec<&Relationship> = state.relationships.iter().filter(|r| !r.pending).collect();
    let Some(mediation) = &state.mediation else {
        return open.len();
    };
    let Ok(mediator) = Mediator::resolve(&mediation.mediator).await else {
        return open.len();
    };

    let mut unreached = 0;
    for relationship in open {
        let sent = match Peer::pairwise(seed, &mediator.did, &relationship.origin) {
            Ok(from) => chat::send_profile(
                &mediator,
                &from,
                &relationship.theirs,
                state.profile.as_deref(),
                false,
            )
            .await
            .is_ok(),
            Err(_) => false,
        };
        if !sent {
            unreached += 1;
        }
    }
    unreached
}

fn find<'a>(
    relationships: &'a [Relationship],
    id: &str,
) -> Result<&'a Relationship, MessagingError> {
    relationships
        .iter()
        .find(|r| conversation::id(&r.ours) == id)
        .ok_or(MessagingError::ContactUnknown)
}

fn find_mut<'a>(
    relationships: &'a mut [Relationship],
    id: &str,
) -> Result<&'a mut Relationship, MessagingError> {
    relationships
        .iter_mut()
        .find(|r| conversation::id(&r.ours) == id)
        .ok_or(MessagingError::ContactUnknown)
}

/// Replaces the relationship opened with the same DID, or adds it.
fn upsert(relationships: &mut Vec<Relationship>, relationship: Relationship) {
    match relationships
        .iter_mut()
        .find(|r| r.origin == relationship.origin)
    {
        Some(existing) => *existing = relationship,
        None => relationships.push(relationship),
    }
}

/// The relationships, the one with the latest activity first.
fn listed(relationships: &[Relationship]) -> Vec<Contact> {
    let mut contacts: Vec<Contact> = relationships.iter().map(Contact::from).collect();
    contacts.sort_by_key(|contact| {
        std::cmp::Reverse(
            contact
                .last
                .as_ref()
                .map_or(contact.since, |l| l.at.max(contact.since)),
        )
    });
    contacts
}

fn seed(held: &Held) -> Result<Zeroizing<[u8; 64]>, MessagingError> {
    held.seed().ok_or(MessagingError::Locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where the ignored tests find a mediator: start one with
    /// `ALMENA_PUBLIC_URL=http://localhost:8080 task dev:memory` in
    /// `../mediator` (or set `ALMENA_TEST_MEDIATOR`), then
    /// `cargo test -- --ignored`.
    fn address() -> String {
        std::env::var("ALMENA_TEST_MEDIATOR").unwrap_or_else(|_| "http://localhost:8080".to_owned())
    }

    #[test]
    fn a_link_is_told_apart_before_anything_is_done_with_it() {
        let card = "did:peer:2.Vz6MkCard";
        assert_eq!(invitation_kind(contacts::link(card)), "contact");
        let mediator = format!(
            "https://mediator.example.com/oob?_oob={}",
            almena_didcomm::b64::encode(
                json!({
                    "type": "https://didcomm.org/out-of-band/2.0/invitation",
                    "id": "x",
                    "from": "did:web:mediator.example.com",
                    "body": {"goal_code": "request-mediate"},
                })
                .to_string()
            )
        );
        assert_eq!(invitation_kind(mediator), "mediator");
        // An address is somewhere to connect to by hand, not an invitation.
        assert_eq!(
            invitation_kind("https://mediator.example.com".into()),
            "unknown"
        );
        assert_eq!(invitation_kind("hello".into()), "unknown");
    }

    #[tokio::test]
    #[ignore = "needs a mediator running"]
    async fn a_wallet_gets_mediation_and_asks_what_is_waiting() {
        let seed = [11u8; 64];

        let mediation = mediate(&seed, &address()).await.expect("mediation");
        assert!(mediation.inbox.starts_with("did:peer:2"));

        // Asking again is the same mediation, and the inbox is already there.
        let again = mediate(&seed, &address()).await.expect("mediation again");
        assert_eq!(again, mediation);

        let waiting = waiting(&seed, &mediation.mediator)
            .await
            .expect("pickup status");
        assert_eq!(waiting.message_count, 0);
    }

    /// Needs the mediator started with `ALMENA_TURN_URLS` and
    /// `ALMENA_TURN_SECRET` set.
    #[tokio::test]
    #[ignore = "needs a mediator running, with TURN"]
    async fn a_mediated_wallet_gets_turn_credentials() {
        let seed = [12u8; 64];
        let mediation = mediate(&seed, &address()).await.expect("mediation");
        let mediator = Mediator::resolve(&mediation.mediator).await.unwrap();
        let inbox = Peer::inbox(&seed, &mediator.did).unwrap();

        let servers = call::ice_servers(&mediator, &inbox)
            .await
            .expect("TURN credentials");
        let server = &servers[0];
        assert!(server["urls"][0].as_str().unwrap().starts_with("turn"));
        assert!(server["username"].as_str().unwrap().contains(':'));
        assert!(server["credential"].is_string());
    }

    /// Two wallets on the same mediator: Bob accepts Alice's invitation, and
    /// after each has synced they know each other by pairwise DIDs only.
    #[tokio::test]
    #[ignore = "needs a mediator running"]
    async fn two_wallets_open_a_relationship_through_an_invitation() {
        let (alice_seed, bob_seed) = ([21u8; 64], [22u8; 64]);
        let mut alice = state::State {
            mediation: Some(mediate(&alice_seed, &address()).await.expect("alice")),
            profile: Some("Alice".into()),
            relationships: Vec::new(),
        };
        let mut bob = state::State {
            mediation: Some(mediate(&bob_seed, &address()).await.expect("bob")),
            profile: Some("Bob".into()),
            relationships: Vec::new(),
        };
        let mediator = alice.mediation.clone().expect("mediation").mediator;

        let link = invitation(&alice_seed, &mediator)
            .await
            .expect("invitation");
        let accepted = accept(&bob_seed, &mut bob, &contacts::read(&link).expect("read"))
            .await
            .expect("accepted");
        assert!(accepted.pending);

        assert_eq!(
            sync(&alice_seed, &mut alice)
                .await
                .expect("alice syncs")
                .changed,
            1
        );
        // Bob reads the ping-response and Alice's name, and sends his back.
        assert_eq!(
            sync(&bob_seed, &mut bob).await.expect("bob syncs").changed,
            2
        );

        let (a, b) = (&alice.relationships[0], &bob.relationships[0]);
        assert!(!a.pending && !b.pending);
        assert_eq!(a.theirs, b.ours);
        assert_eq!(b.theirs, a.ours);
        // Neither ended up speaking to the other's card or inbox.
        let card = Peer::card(&alice_seed, &mediator).expect("card").did;
        assert_ne!(b.theirs, card);

        assert_eq!(b.name.as_deref(), Some("Alice"));

        // Alice reads Bob's name; then nothing is left behind, and another
        // sync changes nothing.
        assert_eq!(
            sync(&alice_seed, &mut alice).await.expect("names").changed,
            1
        );
        assert_eq!(alice.relationships[0].name.as_deref(), Some("Bob"));
        assert_eq!(
            sync(&alice_seed, &mut alice).await.expect("again").changed,
            0
        );
        assert_eq!(
            waiting(&bob_seed, &mediator)
                .await
                .expect("status")
                .message_count,
            0
        );

        // Text, both ways, each from the pairwise the other knows.
        let mediation = alice.mediation.clone().expect("mediation");
        deliver(
            &alice_seed,
            &mediation,
            &alice.relationships[0],
            chat::text("hola, Bob"),
        )
        .await
        .expect("alice writes");
        let arrived = sync(&bob_seed, &mut bob).await.expect("bob reads").arrivals;
        assert_eq!(arrived.len(), 1);
        assert_eq!(arrived[0].entry.content, "hola, Bob");
        assert_eq!(arrived[0].ours, bob.relationships[0].ours);

        let mediation = bob.mediation.clone().expect("mediation");
        deliver(
            &bob_seed,
            &mediation,
            &bob.relationships[0],
            chat::text("hola, Alice"),
        )
        .await
        .expect("bob writes");
        let arrived = sync(&alice_seed, &mut alice)
            .await
            .expect("alice reads")
            .arrivals;
        assert_eq!(arrived.len(), 1);
        assert!(!arrived[0].entry.mine);

        // A retried message keeps its id, so the second copy is recognised.
        let mut first = chat::text("twice");
        let id = first.id.clone();
        let mut second = chat::text("twice");
        second.id = id.clone();
        first.created_time = second.created_time;
        for message in [first, second] {
            deliver(
                &alice_seed,
                &mediation_of(&alice),
                &alice.relationships[0],
                message,
            )
            .await
            .expect("sent");
        }
        let arrived = sync(&bob_seed, &mut bob).await.expect("bob reads").arrivals;
        assert!(arrived.iter().all(|a| a.entry.id == id));
    }

    /// Bob goes live; what Alice sends then is pushed down his socket, and
    /// once he acknowledges it nothing is left queued.
    #[tokio::test]
    #[ignore = "needs a mediator running"]
    async fn a_live_socket_is_handed_what_is_sent() {
        let (alice_seed, bob_seed) = ([31u8; 64], [32u8; 64]);
        let (mut alice, mut bob) = (
            state::State {
                mediation: Some(mediate(&alice_seed, &address()).await.expect("alice")),
                ..Default::default()
            },
            state::State {
                mediation: Some(mediate(&bob_seed, &address()).await.expect("bob")),
                ..Default::default()
            },
        );
        let mediator = mediation_of(&alice).mediator;
        let link = invitation(&alice_seed, &mediator)
            .await
            .expect("invitation");
        accept(&bob_seed, &mut bob, &contacts::read(&link).expect("read"))
            .await
            .expect("accepted");
        sync(&alice_seed, &mut alice).await.expect("alice syncs");
        sync(&bob_seed, &mut bob).await.expect("bob syncs");
        sync(&alice_seed, &mut alice).await.expect("alice again");

        let resolved = Mediator::resolve(&mediator).await.expect("mediator");
        assert!(resolved.socket.is_some());
        let inbox = Peer::inbox(&bob_seed, &resolved.did).expect("inbox");
        let card = Peer::card(&bob_seed, &resolved.did).expect("card");
        let mut socket = live::Socket::open(&resolved, &inbox).await.expect("live");

        deliver(
            &alice_seed,
            &mediation_of(&alice),
            &alice.relationships[0],
            chat::text("en directo"),
        )
        .await
        .expect("alice writes");

        let deliveries = socket.deliveries(&resolved, &inbox).await.expect("pushed");
        let mut outcome = Outcome::default();
        let received = handle(
            &bob_seed,
            &mut bob,
            &resolved,
            &inbox,
            &card,
            deliveries,
            &mut outcome,
        )
        .await
        .expect("handled");
        assert_eq!(outcome.arrivals.len(), 1);
        assert_eq!(outcome.arrivals[0].entry.content, "en directo");

        socket
            .acknowledge(&resolved, &inbox, &received)
            .await
            .expect("acknowledged");
        // The acknowledgement is answered on the socket; by then it is done.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert_eq!(
            waiting(&bob_seed, &mediator)
                .await
                .expect("status")
                .message_count,
            0
        );
    }

    fn mediation_of(state: &state::State) -> Mediation {
        state.mediation.clone().expect("mediation")
    }
}
