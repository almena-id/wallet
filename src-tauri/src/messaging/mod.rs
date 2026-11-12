//! Messaging: the wallet's mediations with DIDComm mediators, and the
//! relationships it has through them.
//!
//! A wallet has no address of its own, so it gets one from a mediator: it asks
//! for mediation (Coordinate Mediation 3.0), registers the DIDs it can be
//! reached at, and from then on anybody can reach one of them by wrapping a
//! message in a `forward` to the mediator, which queues it until the wallet
//! picks it up (Message Pickup 3.0). The protocols, and what the mediator
//! requires of them, are in the mediator's SPEC.md (§3 and §6).
//!
//! **One mediator, and changing it.** A wallet has one mediator. Each
//! relationship records the mediator its pairwise was derived for
//! ([`peer`]), so choosing another moves them: the wallet registers the
//! pairwise it gets at the new one and tells the counterparty with a message
//! from it carrying `from_prior` — a JWT the old pairwise signs, DIDComm's
//! DID rotation — and keeps collecting the old mediator for
//! [`state::GRACE_SECS`], so nothing sent before the counterparty learned of
//! the move is lost. A counterparty's move reaches this wallet the same way:
//! any message whose `from_prior` the counterparty's current DID signed moves
//! the relationship to the DID that sent it. The state can hold more than
//! one mediation only while one is being left.
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
//!
//! Everything here needs the wallet open: every key and the state key come
//! from the seed, which is only held while it is.

pub(crate) mod backup;
mod chat;
mod contacts;
mod conversation;
pub mod live;
pub(crate) mod mediator;
pub(crate) mod notice;
mod peer;
mod photo;
mod push;
pub(crate) mod state;

use almena_didcomm::{unpack, InMemorySecrets, Message};
use serde::{Serialize, Serializer};
use serde_json::json;
use tauri::async_runtime::Mutex;
use tauri::{Manager, Runtime, State};
use zeroize::Zeroizing;

use crate::identity::Held;
use conversation::Entry;
use mediator::Mediator;
use peer::Peer;
use state::{Last, Mediation, Previous, Relationship, GRACE_SECS};

const STATUS_REQUEST: &str = "https://didcomm.org/messagepickup/3.0/status-request";

/// How many `delivery-request`s one sync makes at most.
const SYNC_ROUNDS: usize = 10;

/// Who a message written to one of this wallet's pairwise DIDs is from.
#[derive(Debug, PartialEq, Eq)]
enum Sender {
    /// The counterparty of that relationship (its index), from their current
    /// pairwise DID.
    Theirs(usize),
    /// Somebody else, while the relationship is still pending: their pairwise
    /// is not known until the rotation in their `ping-response` is read, so
    /// a message that overtook it is left queued for the round after.
    NotYetKnown,
    /// Nobody this wallet speaks with there: no relationship has that DID, or
    /// it is open and the message is not from its counterparty. Acknowledged
    /// and dropped, rather than downloaded again on every sync.
    Nobody,
}

/// The rule every message inside a relationship is taken by: it is for one of
/// our pairwise DIDs (`to`) and from that relationship's counterparty.
fn sender(relationships: &[Relationship], to: &[String], from: Option<&str>) -> Sender {
    let Some(at) = relationships.iter().position(|r| r.is_for(to)) else {
        return Sender::Nobody;
    };
    let relationship = &relationships[at];
    if from == Some(relationship.theirs.as_str()) {
        Sender::Theirs(at)
    } else if relationship.pending {
        Sender::NotYetKnown
    } else {
        Sender::Nobody
    }
}

/// How many relationships opened through the card, with nothing said in
/// them yet, a day may bring before strangers' pings are dropped.
const WELCOMES: usize = 20;

/// The relationships opened through the card in the last day that nothing
/// has been said in — what `WELCOMES` counts.
fn silent_welcomes(relationships: &[Relationship], now: u64) -> usize {
    relationships
        .iter()
        .filter(|r| r.welcomed && r.last.is_none() && r.since + 86_400 > now)
        .count()
}

/// How far ahead of this device's clock a message's `created_time` is taken,
/// in seconds; anything later is dated now plus this.
const CLOCK_SKEW: u64 = 300;

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
    /// The mediator takes no new wallets for now (`mediate-deny`): it is at
    /// capacity. The wallets it already serves keep working.
    MediatorFull,
    /// The mediator is private: it takes only the wallets its administrators
    /// invite, and this request answered no invitation of theirs.
    MediatorPrivate,
    /// The counterparty could not be written to: its DID did not resolve,
    /// named no mailbox, or the mailbox did not take the message.
    CounterpartyUnreachable,
    /// Asked for something that needs a mediation this wallet does not have.
    NotConnected,
    /// The mailbox holds as many DIDs as its mediator allows: no new contact
    /// fits there.
    MailboxFull,
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
            Self::MediatorFull => "messaging_mediator_full",
            Self::MediatorPrivate => "messaging_mediator_private",
            Self::CounterpartyUnreachable => "messaging_counterparty_unreachable",
            Self::NotConnected => "messaging_not_connected",
            Self::MailboxFull => "messaging_mailbox_full",
            Self::Pending => "messaging_pending",
            Self::ContactUnknown => "messaging_contact_unknown",
            Self::MessageInvalid => "messaging_message_invalid",
            Self::PhotoInvalid => "messaging_photo_invalid",
            Self::Keys => "messaging_keys",
            Self::Unreadable => "messaging_unreadable",
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

/// Serialises everything that reads the state and writes it back, so a sync
/// and an accepted invitation cannot both start from the same copy. Held
/// across the network calls in between, which is why it is an async lock.
#[derive(Default)]
pub struct Gate(Mutex<()>);

/// The mediation as the interface shows it, with the one being left after
/// a change, while it is.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediatorStatus {
    /// The default mediator's DID, when there is a mediation.
    pub mediator: Option<String>,
    /// The inbox DID registered with it.
    pub inbox: Option<String>,
    /// Every mediation, the current one first.
    pub mediators: Vec<MediatorEntry>,
}

/// One mediation in the list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediatorEntry {
    pub mediator: String,
    pub inbox: String,
    /// The wallet's mediator: not one being left.
    pub default: bool,
    /// Being left: still collected until then, its relationships moving away.
    pub leaving: Option<u64>,
    /// Relationships that go through it.
    pub contacts: usize,
    /// The addresses (DIDs) this wallet holds there: its inbox, its contact
    /// card and one per contact.
    pub dids: u64,
    /// How many its mediator lets it hold. Unknown for a mediator that does
    /// not say.
    pub max_dids: Option<u64>,
}

impl From<&state::State> for MediatorStatus {
    fn from(state: &state::State) -> Self {
        let default = state.default_mediation();
        Self {
            mediator: default.map(|m| m.mediator.clone()),
            inbox: default.map(|m| m.inbox.clone()),
            mediators: state
                .mediations
                .iter()
                .map(|m| MediatorEntry {
                    mediator: m.mediator.clone(),
                    inbox: m.inbox.clone(),
                    default: default.is_some_and(|d| d.mediator == m.mediator),
                    leaving: m.leaving,
                    // The card counts whether or not it was shown yet: its
                    // place is kept for it.
                    dids: 2 + state
                        .relationships
                        .iter()
                        .filter(|r| r.mediator == m.mediator)
                        .count() as u64,
                    max_dids: m.max_dids,
                    contacts: state
                        .relationships
                        .iter()
                        .filter(|r| r.mediator == m.mediator)
                        .count(),
                })
                .collect(),
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
    /// The mediator it goes through.
    pub mediator: String,
}

impl From<&Relationship> for Contact {
    fn from(relationship: &Relationship) -> Self {
        let fingerprint = contacts::name(&relationship.origin);
        Self {
            id: relationship.id(),
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
            mediator: relationship.mediator.clone(),
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

/// The mediation this wallet has. Reads the device only.
#[tauri::command]
pub fn mediator_status<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<MediatorStatus, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;
    Ok(MediatorStatus::from(&state))
}

/// Connects to the mediator somebody pasted — asks for mediation, registers
/// the inbox with it — and makes it the wallet's mediator. A wallet has one:
/// when it had another, its relationships move to the new one (as far as
/// each counterparty can be told now; the rest are tried again on every
/// sync) and the old one is still collected for [`GRACE_SECS`], so nothing
/// sent there before the move is lost.
///
/// Asking again is harmless: the mediator grants the same mediation to the same
/// inbox and answers `no_change` to a DID it already has. A mediator being
/// left that is chosen again is simply kept.
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
    let now = almena_didcomm::message::now();
    let previous: Vec<String> = state
        .mediations
        .iter()
        .filter(|m| m.leaving.is_none() && m.mediator != mediation.mediator)
        .map(|m| m.mediator.clone())
        .collect();
    state
        .mediations
        .retain(|m| m.mediator != mediation.mediator);
    for old in &mut state.mediations {
        old.leaving.get_or_insert(now + GRACE_SECS);
    }
    state.mediations.insert(0, mediation);
    for old in &previous {
        relocate_all(&seed, &mut state, old, now).await;
    }
    state::write(&app, &seed, &state)?;

    // Live delivery gave up for want of a mediation, or does not follow the
    // new one yet: without this nothing arrives live from it until the
    // wallet next comes to the front.
    app.state::<live::Live>().restart(app.clone());

    Ok(MediatorStatus::from(&state))
}

/// Asks the mediators how many messages they are holding for this wallet, in
/// all — the one being left included; the ones that do not answer are left
/// out.
#[tauri::command]
pub async fn mediator_check<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<Waiting, MessagingError> {
    let seed = seed(&held)?;
    let state = state::read(&app, &seed)?;
    if state.mediations.is_empty() {
        return Err(MessagingError::NotConnected);
    }
    let mut total = None;
    let mut failure = MessagingError::MediatorUnreachable;
    for mediation in &state.mediations {
        match waiting(&seed, mediation).await {
            Ok(waiting) => *total.get_or_insert(0) += waiting.message_count,
            Err(error) => failure = error,
        }
    }
    total
        .map(|message_count| Waiting { message_count })
        .ok_or(failure)
}

/// Leaves the mediator: takes the inbox off its recipients if it answers, and
/// forgets the mediation either way — a mediator that is gone must not keep a
/// wallet from choosing another. Its relationships have nowhere to go: they
/// stay, unreachable, until another mediator is chosen and they move there.
/// One still being left after a change goes on being collected.
#[tauri::command]
pub async fn mediator_disconnect<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<MediatorStatus, MessagingError> {
    let seed = seed(&held)?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(&app, &seed)?;
    let at = state
        .mediations
        .iter()
        .position(|m| m.leaving.is_none())
        .ok_or(MessagingError::NotConnected)?;
    let mediation = state.mediations.remove(at);
    if let Ok(mediator) = Mediator::resolve(&mediation.mediator).await {
        if let Ok(inbox) = Peer::inbox(&seed, &mediator.did) {
            let _ = push::register(&mediator, &inbox, None).await;
            let _ = mediator.recipient(&inbox, &inbox, "remove").await;
        }
    }
    if state.mediations.is_empty() {
        push::forget(&app);
    }
    state::write(&app, &seed, &state)?;
    app.state::<live::Live>().restart(app.clone());
    Ok(MediatorStatus::from(&state))
}

/// Registers this device's push token with every mediation, asking for
/// permission to notify the first time. Returns whether a mediator will now
/// notify it: not on desktop, not without a mediation, not when permission was
/// refused, and not with mediators that do not push for this platform.
#[tauri::command]
pub async fn push_register<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<bool, MessagingError> {
    let seed = seed(&held)?;
    let mediations = state::read(&app, &seed)?.mediations;
    if mediations.is_empty() {
        return Ok(false);
    }
    let Some(token) = push::token(&app).await else {
        return Ok(false);
    };
    let mut registered = false;
    let mut failure = None;
    for mediation in &mediations {
        let result = async {
            let mediator = Mediator::resolve(&mediation.mediator).await?;
            let inbox = Peer::inbox(&seed, &mediator.did)?;
            mediator.ensure(&inbox, None).await?;
            push::register(&mediator, &inbox, Some(&token)).await
        }
        .await;
        match result {
            Ok(()) => registered = true,
            Err(MessagingError::MediatorRefused) => {}
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        Some(error) if !registered => Err(error),
        _ => Ok(registered),
    }
}

/// Takes this device off every mediation's pushes, as far as the mediators
/// can be reached, and tells the system no more are wanted. For signing out:
/// an identity that has left the device must not keep waking it.
#[tauri::command]
pub async fn push_unregister<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
) -> Result<(), MessagingError> {
    let seed = seed(&held)?;
    for mediation in state::read(&app, &seed)?.mediations {
        if let Ok(mediator) = Mediator::resolve(&mediation.mediator).await {
            if let Ok(inbox) = Peer::inbox(&seed, &mediator.did) {
                let _ = push::register(&mediator, &inbox, None).await;
            }
        }
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
        .default_mediation()
        .cloned()
        .ok_or(MessagingError::NotConnected)?;
    Ok(Shown {
        url: invitation(&seed, &mediation).await?,
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
        /// Relationships that go through the current mediator: they move to
        /// the new one.
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
        let own = state.mediations.iter().any(|mediation| {
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
            let current = state.default_mediation().map(|m| m.mediator.clone());
            return Ok(LinkDetails::Mediator {
                contacts: state
                    .relationships
                    .iter()
                    .filter(|r| Some(&r.mediator) == current.as_ref())
                    .count(),
                mediator: target.did,
                current,
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
        notice: None,
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

/// Registers the lock the commands that change the state hold, and the live
/// session.
pub fn manage<R: Runtime>(app: &tauri::AppHandle<R>) {
    app.manage(Gate::default());
    app.manage(live::Live::default());
}

/// Forgets everything messaging wrote down. Called when the identity leaves the
/// device, by signing out or by running out of PIN attempts.
///
/// **Twice.** Stopping the live session aborts it, but a session caught in the
/// middle of `apply` — under the [`Gate`], between two awaits — still finishes
/// writing what it was writing, which would leave a file sealed under a seed
/// that is gone. So the files go now, and again as soon as the gate is let go
/// of: this is called from commands that cannot wait for it.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(live) = app.try_state::<live::Live>() {
        live.stop();
    }
    forget(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(gate) = app.try_state::<Gate>() {
            let _guard = gate.0.lock().await;
            forget(&app);
        }
    });
}

/// Removes every file messaging keeps.
fn forget<R: Runtime>(app: &tauri::AppHandle<R>) {
    state::clear(app);
    conversation::clear(app);
    crate::credentials::clear(app);
    crate::presented::clear(app);
    photo::clear(app);
}

/// Asks for mediation from the mediator `input` names and registers the inbox
/// with it: the protocol half of [`mediator_connect`], without the state.
async fn mediate(seed: &[u8; 64], input: &str) -> Result<Mediation, MessagingError> {
    let target = mediator::target(input)?;
    let mediator = Mediator::resolve(&target.did).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;

    let max_dids = mediator.ensure(&inbox, target.invitation).await?;

    Ok(Mediation {
        mediator: mediator.did,
        inbox: inbox.did,
        leaving: None,
        max_dids,
    })
}

/// How many messages the mediator is holding for the inbox.
async fn waiting(seed: &[u8; 64], mediation: &Mediation) -> Result<Waiting, MessagingError> {
    let mediator = Mediator::resolve(&mediation.mediator).await?;
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
async fn invitation(seed: &[u8; 64], mediation: &Mediation) -> Result<String, MessagingError> {
    let mediator = Mediator::resolve(&mediation.mediator).await?;
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
        .default_mediation()
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

/// The `did:web` name of a registry DID — the form this wallet's messaging
/// resolves: `did:webvh:{SCID}:host…` is `did:web:host…`.
fn web_name(did: &str) -> String {
    match did.strip_prefix("did:webvh:") {
        Some(rest) => rest
            .split_once(':')
            .map_or_else(|| did.to_owned(), |(_, name)| format!("did:web:{name}")),
        None => did.to_owned(),
    }
}

/// Where an issuer this wallet is pairing with (`registry.rs`, `pair`) may
/// write to it: the pairwise for the issuer's DID, registered with the
/// mediator the relationship with it goes through (the default for a new
/// one), and the relationship it opens — named as the issuer,
/// open from the start, its counterparty the issuer's `did:web` name. Its
/// DID goes in the pairing token; `None` when there is no mediator or it
/// cannot be reached, and the pairing goes on without notices.
pub(crate) async fn issuer_channel<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    issuer: &str,
    name: &str,
) -> Option<String> {
    let gate = app.try_state::<Gate>()?;
    let _guard = gate.0.lock().await;
    let mut state = state::read(app, seed).ok()?;
    let found = state
        .relationships
        .iter()
        .find(|r| r.origin == issuer)
        .filter(|r| state.mediation(&r.mediator).is_some())
        .cloned();
    let through = match &found {
        Some(found) => found.mediator.clone(),
        None => state.default_mediation()?.mediator.clone(),
    };
    let mediator = Mediator::resolve(&through).await.ok()?;
    let inbox = Peer::inbox(seed, &mediator.did).ok()?;
    mediator.ensure(&inbox, None).await.ok()?;
    let pairwise = Peer::pairwise(seed, &mediator.did, issuer).ok()?;
    mediator.register(&inbox, &pairwise).await.ok()?;
    let name: String = name.chars().take(chat::NAME_CHARS).collect();
    let relationship = match found {
        Some(found) => Relationship {
            theirs: web_name(issuer),
            name: Some(name),
            ..found
        },
        None => Relationship {
            origin: issuer.to_owned(),
            name: Some(name),
            ..Relationship::new(&mediator.did, pairwise.did.clone(), web_name(issuer), false)
        },
    };
    upsert(&mut state.relationships, relationship);
    state::write(app, seed, &state).ok()?;
    Some(pairwise.did)
}

/// An issuer's notice in a conversation, and the issuer's DID as this wallet
/// paired with it (the relationship's origin): what collecting needs.
pub(crate) fn notice_of<R: Runtime>(
    app: &tauri::AppHandle<R>,
    seed: &[u8; 64],
    contact: &str,
    entry: &str,
) -> Result<(String, notice::Notice), MessagingError> {
    let state = state::read(app, seed)?;
    let relationship = find(&state.relationships, contact)?;
    conversation::read(app, seed, contact)?
        .into_iter()
        .find(|e| e.id == entry)
        .and_then(|e| e.notice)
        .map(|found| (relationship.origin.clone(), found))
        .ok_or(MessagingError::ContactUnknown)
}

/// A message of text that arrived, for the relationship whose conversation
/// is `id`.
struct Arrival {
    id: String,
    entry: Entry,
}

/// What a sync came to, before the conversations are written.
#[derive(Default)]
struct Outcome {
    /// Relationships opened, confirmed or renamed.
    changed: usize,
    arrivals: Vec<Arrival>,
}

/// Picks up and handles what is waiting at every mediator: the protocol half
/// of [`messages_sync`]. Relationships are changed in `state`; the texts that
/// arrived are returned, for the caller to write into their conversations.
/// Then what a move left behind is settled ([`settle`]).
///
/// A mediator that does not answer is skipped, and what waits there waits
/// for the next sync; when none answers, that is the error.
async fn sync(seed: &[u8; 64], state: &mut state::State) -> Result<Outcome, MessagingError> {
    if state.mediations.is_empty() {
        return Err(MessagingError::NotConnected);
    }
    let mut outcome = Outcome::default();
    let mut reached = false;
    let mut failure = MessagingError::MediatorUnreachable;
    let mediators: Vec<String> = state
        .mediations
        .iter()
        .map(|m| m.mediator.clone())
        .collect();
    for mediator in mediators {
        match sync_at(seed, state, &mediator, &mut outcome).await {
            Ok(()) => reached = true,
            Err(error) => failure = error,
        }
    }
    outcome.changed += settle(seed, state, almena_didcomm::message::now()).await;
    if reached {
        Ok(outcome)
    } else {
        Err(failure)
    }
}

/// Picks up and handles what is waiting at the mediator `mediator`, into
/// `outcome`. When it stops answering after the first round, what was taken
/// so far stays and the rest waits for the next sync.
async fn sync_at(
    seed: &[u8; 64],
    state: &mut state::State,
    mediator: &str,
    outcome: &mut Outcome,
) -> Result<(), MessagingError> {
    let mediator = Mediator::resolve(mediator).await?;
    let inbox = Peer::inbox(seed, &mediator.did)?;
    let card = Peer::card(seed, &mediator.did)?;
    let max_dids = mediator.ensure(&inbox, None).await?;
    // The mediator's limit may have changed since: keep the one it says now.
    if let Some(held) = state
        .mediations
        .iter_mut()
        .find(|m| m.mediator == mediator.did)
        .filter(|m| m.max_dids != max_dids)
    {
        held.max_dids = max_dids;
        outcome.changed += 1;
    }

    for round in 0..SYNC_ROUNDS {
        let deliveries = match mediator.deliveries(&inbox).await {
            Ok(deliveries) => deliveries,
            Err(_) if round > 0 => break,
            Err(error) => return Err(error),
        };
        if deliveries.is_empty() {
            break;
        }

        let received = handle(seed, state, &mediator, &inbox, &card, deliveries, outcome).await?;

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
    Ok(())
}

/// Moves the relationship at `at` to `mediator`: registers its pairwise
/// there, tells the counterparty from it — a profile message carrying
/// `from_prior`, which the pairwise it leaves signs — and keeps the old one
/// collected for [`GRACE_SECS`]. Nothing changes when the counterparty cannot
/// be told.
async fn relocate(
    seed: &[u8; 64],
    state: &mut state::State,
    at: usize,
    mediator: &str,
    now: u64,
) -> Result<(), MessagingError> {
    let relationship = &state.relationships[at];
    if relationship.mediator == mediator {
        return Ok(());
    }
    if relationship.pending {
        return Err(MessagingError::Pending);
    }
    if !state
        .mediation(mediator)
        .is_some_and(|m| m.leaving.is_none())
    {
        return Err(MessagingError::NotConnected);
    }
    let target = Mediator::resolve(mediator).await?;
    let inbox = Peer::inbox(seed, &target.did)?;
    target.ensure(&inbox, None).await?;
    let new = Peer::pairwise(seed, &target.did, &relationship.origin)?;
    target.register(&inbox, &new).await?;

    let old = relationship.pairwise(seed)?;
    let rotation = almena_didcomm::FromPrior::new(&old.did, &new.did)
        .pack(None, target.resolver(), &old.secrets())
        .await
        .map_err(|_| MessagingError::Keys)?;
    let mut message = chat::profile(state.profile.as_deref(), false);
    message.from_prior = Some(rotation);
    contacts::send(&target, &new, &relationship.theirs, message).await?;

    let relationship = &mut state.relationships[at];
    relationship.previous = Some(Previous {
        ours: std::mem::replace(&mut relationship.ours, new.did),
        mediator: std::mem::replace(&mut relationship.mediator, target.did),
        until: now + GRACE_SECS,
    });
    Ok(())
}

/// Moves every relationship that goes through `leaving` to the default
/// mediator, as far as each counterparty can be told now. Returns how many
/// moved.
async fn relocate_all(seed: &[u8; 64], state: &mut state::State, leaving: &str, now: u64) -> usize {
    let Some(to) = state.default_mediation().map(|m| m.mediator.clone()) else {
        return 0;
    };
    let mut moved = 0;
    for at in 0..state.relationships.len() {
        let relationship = &state.relationships[at];
        if relationship.mediator != leaving || relationship.pending {
            continue;
        }
        if relocate(seed, state, at, &to, now).await.is_ok() {
            moved += 1;
        }
    }
    moved
}

/// What moves leave behind, once their time is up: a pairwise left at
/// another mediator is unregistered there and forgotten, and a mediator
/// being left whose relationships have all moved — or that has nothing left
/// to collect — is left for good: its card and inbox unregistered, its
/// pushes off. Relationships still on a mediator being left are moved again
/// first. Returns how many relationships changed.
async fn settle(seed: &[u8; 64], state: &mut state::State, now: u64) -> usize {
    let mut changed = 0;
    let leaving: Vec<String> = state
        .mediations
        .iter()
        .filter(|m| m.leaving.is_some())
        .map(|m| m.mediator.clone())
        .collect();
    for mediator in &leaving {
        changed += relocate_all(seed, state, mediator, now).await;
    }

    for relationship in &mut state.relationships {
        let Some(previous) = relationship
            .previous
            .take_if(|previous| previous.until <= now)
        else {
            continue;
        };
        if let Ok(mediator) = Mediator::resolve(&previous.mediator).await {
            if let (Ok(inbox), Ok(old)) = (
                Peer::inbox(seed, &mediator.did),
                Peer::pairwise(seed, &mediator.did, &relationship.origin),
            ) {
                let _ = mediator.recipient(&inbox, &old, "remove").await;
            }
        }
        changed += 1;
    }

    let mut gone = Vec::new();
    for mediation in &state.mediations {
        let Some(until) = mediation.leaving else {
            continue;
        };
        let still_used = state.relationships.iter().any(|r| {
            r.mediator == mediation.mediator
                || r.previous
                    .as_ref()
                    .is_some_and(|p| p.mediator == mediation.mediator)
        });
        if until > now && still_used {
            continue;
        }
        if let Ok(mediator) = Mediator::resolve(&mediation.mediator).await {
            if let (Ok(inbox), Ok(card)) = (
                Peer::inbox(seed, &mediator.did),
                Peer::card(seed, &mediator.did),
            ) {
                let _ = push::register(&mediator, &inbox, None).await;
                let _ = mediator.recipient(&inbox, &card, "remove").await;
                let _ = mediator.recipient(&inbox, &inbox, "remove").await;
            }
        }
        gone.push(mediation.mediator.clone());
    }
    state.mediations.retain(|m| !gone.contains(&m.mediator));
    changed
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
    // Every DID a message here can be for: the inbox, the card, and the
    // pairwise of each relationship through this mediator — or that left it
    // and is still collected — derived again rather than kept.
    let mut secrets = InMemorySecrets::new();
    inbox.add_to(&mut secrets);
    card.add_to(&mut secrets);
    for relationship in &state.relationships {
        let here = relationship.mediator == mediator.did
            || relationship
                .previous
                .as_ref()
                .is_some_and(|previous| previous.mediator == mediator.did);
        if here {
            Peer::pairwise(seed, &mediator.did, &relationship.origin)?.add_to(&mut secrets);
        }
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

        // DID rotation: the counterparty's current DID signed that the
        // conversation moves to the DID that sent this — the card's answer
        // to an accepted invitation, or a counterparty that moved to another
        // mediator. Whatever the message is, it is read as from the new one.
        if let Some(rotation) = &metadata.from_prior {
            if message.from.as_deref() == Some(rotation.sub.as_str()) {
                if let Some(relationship) = state
                    .relationships
                    .iter_mut()
                    .find(|r| r.is_for(&to) && r.theirs == rotation.iss)
                {
                    relationship.theirs = rotation.sub.clone();
                    outcome.changed += 1;
                }
            }
        }

        match message.type_.as_str() {
            contacts::PING if to.contains(&card.did) => {
                // Whoever holds the invitation may open a relationship, but
                // not without end: past `WELCOMES` new ones in a day that
                // nothing has been said in, a stranger's ping is dropped. One
                // who is already a contact is always answered.
                let known = message
                    .from
                    .as_deref()
                    .is_some_and(|from| state.relationships.iter().any(|r| r.origin == from));
                if !known
                    && silent_welcomes(&state.relationships, almena_didcomm::message::now())
                        >= WELCOMES
                {
                    received.push(id);
                    continue;
                }
                // A sender that cannot be answered now is left queued and
                // tried again on the next sync, without holding up the rest.
                match contacts::welcome(
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
                    Ok(relationship) => {
                        upsert(&mut state.relationships, relationship);
                        outcome.changed += 1;
                        received.push(id);
                    }
                    // No room for another contact here: it will not fit on
                    // a later sync either, so it is dropped, not kept.
                    Err(MessagingError::MailboxFull) => received.push(id),
                    Err(_) => {}
                }
            }
            contacts::PING_RESPONSE => {
                // The card's rotation, read above, moved the conversation to
                // the pairwise that sent this.
                if let Some(relationship) = state.relationships.iter_mut().find(|r| r.is_for(&to)) {
                    if relationship.pending
                        && message.from.as_deref() == Some(relationship.theirs.as_str())
                    {
                        relationship.pending = false;
                        outcome.changed += 1;
                    }
                }
                received.push(id);
            }
            chat::TEXT | chat::PROFILE => {
                let relationship = match sender(&state.relationships, &to, message.from.as_deref())
                {
                    Sender::Theirs(at) => &mut state.relationships[at],
                    Sender::NotYetKnown => continue,
                    Sender::Nobody => {
                        received.push(id);
                        continue;
                    }
                };

                if message.type_ == chat::TEXT {
                    if let Some(content) = chat::read_text(&message) {
                        outcome.arrivals.push(Arrival {
                            id: relationship.id(),
                            entry: Entry {
                                id: message.id.clone(),
                                mine: false,
                                content,
                                // The sender's clock, but never ahead of
                                // ours by more than a little: a date in the
                                // future would pin the conversation on top.
                                at: {
                                    let now = almena_didcomm::message::now();
                                    message
                                        .created_time
                                        .map_or(now, |at| at.min(now + CLOCK_SKEW))
                                },
                                failed: false,
                                notice: None,
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
                        if let Ok(pairwise) = relationship.pairwise(seed) {
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
            notice::STATUS => {
                // An issuer this wallet applied to, over the pairwise named
                // when pairing; from anybody else it is dropped.
                let from_them = state
                    .relationships
                    .iter()
                    .find(|r| r.is_for(&to))
                    .filter(|r| message.from.as_deref() == Some(r.theirs.as_str()));
                if let (Some(relationship), Some(read)) = (from_them, notice::read(&message)) {
                    let now = almena_didcomm::message::now();
                    outcome.arrivals.push(Arrival {
                        id: relationship.id(),
                        entry: Entry {
                            id: message.id.clone(),
                            mine: false,
                            content: String::new(),
                            at: message
                                .created_time
                                .map_or(now, |at| at.min(now + CLOCK_SKEW)),
                            failed: false,
                            notice: Some(read),
                        },
                    });
                }
                received.push(id);
            }
            // What this wallet does not speak is acknowledged and dropped:
            // left queued, it would come back on every sync and every push.
            _ => received.push(id),
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
    let mut failure = None;
    for arrival in outcome.arrivals {
        let Some(relationship) = state
            .relationships
            .iter_mut()
            .find(|r| r.id() == arrival.id)
        else {
            continue;
        };
        let last = Last {
            content: arrival.entry.content.clone(),
            at: arrival.entry.at,
            mine: false,
            notice: arrival.entry.notice.as_ref().map(|n| n.status.clone()),
        };
        match conversation::put(app, seed, &arrival.id, arrival.entry) {
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
    let relationship = find_mut(&mut state.relationships, id)?;
    if relationship.pending {
        return Err(MessagingError::Pending);
    }

    entry.failed = deliver(seed, relationship, message).await.is_err();
    conversation::put(app, seed, id, entry.clone())?;
    relationship.cleared = false;
    if relationship.last.as_ref().is_none_or(|l| l.at <= entry.at) {
        relationship.last = Some(Last {
            content: entry.content.clone(),
            at: entry.at,
            mine: true,
            notice: None,
        });
    }
    state::write(app, seed, state)?;
    Ok(entry)
}

/// Sends `message` to the counterparty of `relationship`, from its pairwise.
async fn deliver(
    seed: &[u8; 64],
    relationship: &Relationship,
    message: Message,
) -> Result<(), MessagingError> {
    let mediator = Mediator::resolve(&relationship.mediator).await?;
    let from = relationship.pairwise(seed)?;
    contacts::send(&mediator, &from, &relationship.theirs, message).await
}

/// Sends this wallet's name to every contact that has answered. Returns how
/// many it could not be sent to.
async fn announce(seed: &[u8; 64], state: &state::State) -> usize {
    let mut unreached = 0;
    for relationship in state.relationships.iter().filter(|r| !r.pending) {
        let sent = match (
            Mediator::resolve(&relationship.mediator).await,
            relationship.pairwise(seed),
        ) {
            (Ok(mediator), Ok(from)) => chat::send_profile(
                &mediator,
                &from,
                &relationship.theirs,
                state.profile.as_deref(),
                false,
            )
            .await
            .is_ok(),
            _ => false,
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
        .find(|r| r.id() == id)
        .ok_or(MessagingError::ContactUnknown)
}

fn find_mut<'a>(
    relationships: &'a mut [Relationship],
    id: &str,
) -> Result<&'a mut Relationship, MessagingError> {
    relationships
        .iter_mut()
        .find(|r| r.id() == id)
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
    #[test]
    fn a_registry_did_is_written_to_by_its_did_web_name() {
        use super::web_name;
        assert_eq!(
            web_name("did:webvh:QmScid:almena.id:ids:idn_1"),
            "did:web:almena.id:ids:idn_1"
        );
        assert_eq!(web_name("did:web:almena.id"), "did:web:almena.id");
    }

    #[test]
    fn a_message_is_taken_only_from_the_counterparty_of_the_pairwise_it_is_for() {
        use super::{sender, Relationship, Sender};
        let mut open = Relationship::new(
            "did:web:m",
            "did:ours:1".into(),
            "did:theirs:1".into(),
            false,
        );
        open.previous = Some(super::Previous {
            ours: "did:before:1".into(),
            mediator: "did:web:old".into(),
            until: 9,
        });
        let pending =
            Relationship::new("did:web:m", "did:ours:2".into(), "did:card:2".into(), true);
        let all = vec![open, pending];
        let to = |did: &str| vec![did.to_owned()];

        assert_eq!(
            sender(&all, &to("did:ours:1"), Some("did:theirs:1")),
            Sender::Theirs(0)
        );
        // To the pairwise it moved away from, while that is collected.
        assert_eq!(
            sender(&all, &to("did:before:1"), Some("did:theirs:1")),
            Sender::Theirs(0)
        );
        // Somebody else writing to an open relationship's pairwise.
        assert_eq!(
            sender(&all, &to("did:ours:1"), Some("did:else")),
            Sender::Nobody
        );
        assert_eq!(sender(&all, &to("did:ours:1"), None), Sender::Nobody);
        // To a pairwise this wallet does not have.
        assert_eq!(
            sender(&all, &to("did:ours:9"), Some("did:theirs:1")),
            Sender::Nobody
        );
        // Pending: their pairwise is not known yet, so it waits.
        assert_eq!(
            sender(&all, &to("did:ours:2"), Some("did:new:2")),
            Sender::NotYetKnown
        );
        assert_eq!(
            sender(&all, &to("did:ours:2"), Some("did:card:2")),
            Sender::Theirs(1)
        );
    }

    #[test]
    fn only_silent_recent_welcomes_count_against_the_card() {
        use super::{silent_welcomes, Relationship};
        let now = 1_800_000_000;
        let fresh = |welcomed: bool| Relationship {
            since: now - 60,
            welcomed,
            ..Relationship::new("did:web:m", "did:ours".into(), "did:theirs".into(), false)
        };
        let mut old = fresh(true);
        old.since = now - 90_000;
        let mut spoken = fresh(true);
        spoken.last = Some(super::Last {
            content: "hi".into(),
            mine: false,
            at: now,
            notice: None,
        });
        let all = vec![fresh(true), fresh(false), old, spoken];
        assert_eq!(silent_welcomes(&all, now), 1);
    }

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
        // An Almena mediator says how many DIDs the mailbox may hold.
        assert!(mediation.max_dids.is_some_and(|max| max > 2));

        // Asking again is the same mediation, and the inbox is already there.
        let again = mediate(&seed, &address()).await.expect("mediation again");
        assert_eq!(again, mediation);

        let waiting = waiting(&seed, &mediation).await.expect("pickup status");
        assert_eq!(waiting.message_count, 0);
    }

    /// Two wallets on the same mediator: Bob accepts Alice's invitation, and
    /// after each has synced they know each other by pairwise DIDs only.
    #[tokio::test]
    #[ignore = "needs a mediator running"]
    async fn two_wallets_open_a_relationship_through_an_invitation() {
        let (alice_seed, bob_seed) = ([21u8; 64], [22u8; 64]);
        let mut alice = state::State {
            mediations: vec![mediate(&alice_seed, &address()).await.expect("alice")],
            profile: Some("Alice".into()),
            relationships: Vec::new(),
        };
        let mut bob = state::State {
            mediations: vec![mediate(&bob_seed, &address()).await.expect("bob")],
            profile: Some("Bob".into()),
            relationships: Vec::new(),
        };
        let mediator = mediation_of(&alice).mediator;

        let link = invitation(&alice_seed, &mediation_of(&alice))
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
        // Bob reads the ping-response — the rotation away from the card, and
        // the relationship confirmed — and Alice's name, and sends his back.
        assert_eq!(
            sync(&bob_seed, &mut bob).await.expect("bob syncs").changed,
            3
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
            waiting(&bob_seed, &mediation_of(&bob))
                .await
                .expect("status")
                .message_count,
            0
        );

        // Text, both ways, each from the pairwise the other knows.
        deliver(
            &alice_seed,
            &alice.relationships[0],
            chat::text("hola, Bob"),
        )
        .await
        .expect("alice writes");
        let arrived = sync(&bob_seed, &mut bob).await.expect("bob reads").arrivals;
        assert_eq!(arrived.len(), 1);
        assert_eq!(arrived[0].entry.content, "hola, Bob");
        assert_eq!(arrived[0].id, bob.relationships[0].id());

        deliver(&bob_seed, &bob.relationships[0], chat::text("hola, Alice"))
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
            deliver(&alice_seed, &alice.relationships[0], message)
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
                mediations: vec![mediate(&alice_seed, &address()).await.expect("alice")],
                ..Default::default()
            },
            state::State {
                mediations: vec![mediate(&bob_seed, &address()).await.expect("bob")],
                ..Default::default()
            },
        );
        let mediator = mediation_of(&alice).mediator;
        let link = invitation(&alice_seed, &mediation_of(&alice))
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
            waiting(&bob_seed, &mediation_of(&bob))
                .await
                .expect("status")
                .message_count,
            0
        );
    }

    /// The second mediator the move test needs: start another with
    /// `ALMENA_PORT=8081 ALMENA_PUBLIC_URL=http://localhost:8081 task
    /// dev:memory` (or set `ALMENA_TEST_MEDIATOR_2`).
    fn second_address() -> String {
        std::env::var("ALMENA_TEST_MEDIATOR_2")
            .unwrap_or_else(|_| "http://localhost:8081".to_owned())
    }

    /// Alice moves her relationship with Bob to a second mediator: what Bob
    /// sends before he learns of it still reaches her at the first, the
    /// rotation moves him to her new pairwise, and once the grace is over
    /// the old one is let go.
    #[tokio::test]
    #[ignore = "needs two mediators running"]
    async fn a_relationship_moves_to_another_mediator() {
        let (alice_seed, bob_seed) = ([41u8; 64], [42u8; 64]);
        let mut alice = state::State {
            mediations: vec![mediate(&alice_seed, &address()).await.expect("alice")],
            ..Default::default()
        };
        let mut bob = state::State {
            mediations: vec![mediate(&bob_seed, &address()).await.expect("bob")],
            ..Default::default()
        };
        let link = invitation(&alice_seed, &mediation_of(&alice))
            .await
            .expect("invitation");
        accept(&bob_seed, &mut bob, &contacts::read(&link).expect("read"))
            .await
            .expect("accepted");
        sync(&alice_seed, &mut alice).await.expect("alice syncs");
        sync(&bob_seed, &mut bob).await.expect("bob syncs");
        sync(&alice_seed, &mut alice).await.expect("alice again");
        let first = mediation_of(&alice).mediator;
        let before = alice.relationships[0].ours.clone();

        // A second mediator becomes the default; the relationship stays.
        let second = mediate(&alice_seed, &second_address())
            .await
            .expect("second mediation");
        alice.mediations.insert(0, second.clone());
        assert_eq!(alice.relationships[0].mediator, first);

        let now = almena_didcomm::message::now();
        relocate(&alice_seed, &mut alice, 0, &second.mediator, now)
            .await
            .expect("moved");
        let moved = alice.relationships[0].clone();
        assert_eq!(moved.mediator, second.mediator);
        assert_ne!(moved.ours, before);
        assert_eq!(
            moved.previous.as_ref().map(|p| p.ours.as_str()),
            Some(before.as_str())
        );
        // No key in common between the pairwise it left and the new one.
        assert_eq!(
            moved.ours,
            Peer::pairwise(&alice_seed, &second.mediator, &moved.origin)
                .expect("pairwise")
                .did
        );

        // Bob has not synced: he still writes to the old pairwise, and it
        // still reaches Alice, at the first mediator.
        assert_eq!(bob.relationships[0].theirs, before);
        deliver(&bob_seed, &bob.relationships[0], chat::text("¿sigues ahí?"))
            .await
            .expect("bob writes to the old one");
        let arrived = sync(&alice_seed, &mut alice).await.expect("alice reads");
        assert_eq!(arrived.arrivals.len(), 1);
        assert_eq!(arrived.arrivals[0].id, moved.id());

        // Bob reads the rotation: from now on he writes to the new pairwise.
        sync(&bob_seed, &mut bob).await.expect("bob reads the move");
        assert_eq!(bob.relationships[0].theirs, moved.ours);
        deliver(&bob_seed, &bob.relationships[0], chat::text("ya te veo"))
            .await
            .expect("bob writes to the new one");
        let mut outcome = Outcome::default();
        sync_at(&alice_seed, &mut alice, &second.mediator, &mut outcome)
            .await
            .expect("alice reads at the second");
        assert_eq!(outcome.arrivals.len(), 1);
        assert_eq!(outcome.arrivals[0].entry.content, "ya te veo");
        // And Alice writes from the new one, which Bob takes.
        deliver(&alice_seed, &alice.relationships[0], chat::text("perfecto"))
            .await
            .expect("alice writes");
        let arrived = sync(&bob_seed, &mut bob).await.expect("bob reads");
        assert_eq!(arrived.arrivals.len(), 1);

        // After the grace, the old pairwise is let go.
        assert_eq!(settle(&alice_seed, &mut alice, now + GRACE_SECS).await, 1);
        assert_eq!(alice.relationships[0].previous, None);

        // The first mediator taken off the list: nothing goes through it
        // any more, so it is left for good at once.
        alice.mediations[1].leaving = Some(now + GRACE_SECS);
        settle(&alice_seed, &mut alice, now).await;
        assert_eq!(alice.mediations.len(), 1);
        assert_eq!(alice.mediations[0].mediator, second.mediator);
    }

    fn mediation_of(state: &state::State) -> Mediation {
        state.default_mediation().cloned().expect("mediation")
    }
}
