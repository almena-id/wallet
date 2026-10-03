//! Contacts: opening a relationship with somebody, and the first messages it
//! takes.
//!
//! **The invitation names the contact card, never a pairwise.** The card is a
//! long-lived DID of its own (see [`super::peer`]) that the wallet shows as an
//! Out-of-Band 2.0 invitation — as a code, as a link, and one day in a
//! directory. Whoever accepts it never speaks to the card for long:
//!
//! 1. They derive a pairwise for the card, register it with their mediator and
//!    send a Trust Ping from it to the card, with the invitation as `pthid`.
//! 2. This wallet picks it up, derives a pairwise for them, registers it, and
//!    answers from it with a `ping-response` that carries `from_prior` — a JWT
//!    the card signs, saying the conversation moves to that pairwise.
//! 3. They check the rotation against the card they were invited by, and from
//!    then on each side knows only the other's pairwise.
//!
//! So the card links nothing: everybody who used it is answered from a
//! different DID, and nobody learns another's.

use almena_didcomm::{b64, FromPrior, Message, PackOptions, Urgency};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use url::Url;

use super::chat;
use super::mediator::{self, Mediator};
use super::peer::Peer;
use super::state::Relationship;
use super::MessagingError;

const INVITATION: &str = "https://didcomm.org/out-of-band/2.0/invitation";
pub const PING: &str = "https://didcomm.org/trust-ping/2.0/ping";
pub const PING_RESPONSE: &str = "https://didcomm.org/trust-ping/2.0/ping-response";

/// The goal of the wallet's own invitation. A mediator's says
/// `request-mediate`, and is not one to answer from here.
const CONNECT: &str = "connect";
const REQUEST_MEDIATE: &str = "request-mediate";

/// The scheme the invitation link is written with: this wallet's own, which it
/// registers with the system so the link opens it. Reading does not depend on
/// it — any link with an `_oob` parameter is read, `didcomm://` and `https://`
/// from other wallets included.
const LINK: &str = "almena://invite";

/// An invitation somebody pasted: who to write to, and in which thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    pub id: String,
    pub from: String,
}

/// The wallet's own invitation, as a link, for the card `card`.
///
/// The invitation never changes for a card — its `id` is derived from it — so
/// a code that was printed or published keeps working.
pub fn link(card: &str) -> String {
    let invitation = json!({
        "type": INVITATION,
        "id": invitation_id(card),
        "from": card,
        "body": {"goal_code": CONNECT, "accept": ["didcomm/v2"]},
    });
    format!("{LINK}?_oob={}", b64::encode(invitation.to_string()))
}

/// Reads an invitation link: any URL with an `_oob` parameter whose invitation
/// is not a mediator's.
///
/// # Errors
///
/// [`MessagingError::InvitationUnreadable`] for anything else.
pub fn read(input: &str) -> Result<Invitation, MessagingError> {
    let url = Url::parse(input.trim()).map_err(|_| MessagingError::InvitationUnreadable)?;
    let (_, encoded) = url
        .query_pairs()
        .find(|(name, _)| name == "_oob")
        .ok_or(MessagingError::InvitationUnreadable)?;
    let bytes = b64::decode(encoded.trim_end_matches('='))
        .map_err(|_| MessagingError::InvitationUnreadable)?;
    let invitation: Value =
        serde_json::from_slice(&bytes).map_err(|_| MessagingError::InvitationUnreadable)?;

    if invitation["type"] != INVITATION || invitation["body"]["goal_code"] == REQUEST_MEDIATE {
        return Err(MessagingError::InvitationUnreadable);
    }
    let from = invitation["from"]
        .as_str()
        .filter(|from| from.starts_with("did:"))
        .ok_or(MessagingError::InvitationUnreadable)?;
    let id = invitation["id"]
        .as_str()
        .ok_or(MessagingError::InvitationUnreadable)?;

    Ok(Invitation {
        id: id.to_owned(),
        from: from.to_owned(),
    })
}

/// A short name for a relationship until it has a better one: a fingerprint of
/// the DID it was opened with, the same on every device.
pub fn name(origin: &str) -> String {
    let hash = Sha256::digest(origin.as_bytes());
    format!(
        "{:02X}{:02X}·{:02X}{:02X}",
        hash[0], hash[1], hash[2], hash[3]
    )
}

/// Accepts an invitation: registers a pairwise for it and says hello from it.
///
/// Accepting the same invitation twice returns the relationship it opened.
///
/// # Errors
///
/// [`MessagingError::InvitationOwn`] for this wallet's own invitation, and the
/// mediator's and the counterparty's errors when either cannot be reached.
pub async fn accept(
    seed: &[u8; 64],
    mediator: &Mediator,
    inbox: &Peer,
    card: &Peer,
    invitation: &Invitation,
    relationships: &[Relationship],
) -> Result<Relationship, MessagingError> {
    if invitation.from == card.did {
        return Err(MessagingError::InvitationOwn);
    }
    if let Some(existing) = relationships.iter().find(|r| r.origin == invitation.from) {
        return Ok(existing.clone());
    }

    let pairwise = Peer::pairwise(seed, &mediator.did, &invitation.from)?;
    mediator.register(inbox, &pairwise).await?;

    let mut ping = Message::new(PING, json!({"response_requested": true}));
    ping.pthid = Some(invitation.id.clone());
    send(mediator, &pairwise, &invitation.from, ping).await?;

    Ok(Relationship::new(
        pairwise.did,
        invitation.from.clone(),
        true,
    ))
}

/// Somebody wrote to the card: gives them a pairwise, registers it, and
/// answers from it with the rotation away from the card — then sends this
/// wallet's name, `profile`, and asks for theirs.
///
/// Returns the relationship, new or the one they already had.
pub async fn welcome(
    seed: &[u8; 64],
    mediator: &Mediator,
    inbox: &Peer,
    card: &Peer,
    ping: &Message,
    relationships: &[Relationship],
    profile: Option<&str>,
) -> Result<Relationship, MessagingError> {
    let sender = ping
        .from
        .as_deref()
        .ok_or(MessagingError::InvitationUnreadable)?;
    let pairwise = Peer::pairwise(seed, &mediator.did, sender)?;
    mediator.register(inbox, &pairwise).await?;

    let rotation = FromPrior::new(&card.did, &pairwise.did)
        .pack(None, mediator.resolver(), &card.secrets())
        .await
        .map_err(|_| MessagingError::Keys)?;
    let mut response = Message::new(PING_RESPONSE, json!({}));
    response.thid = Some(ping.id.clone());
    response.from_prior = Some(rotation);
    send(mediator, &pairwise, sender, response).await?;
    // The relationship is open whether or not this arrives; a name is only
    // missing until the next time it is sent.
    let _ = chat::send_profile(mediator, &pairwise, sender, profile, true).await;

    Ok(relationships
        .iter()
        .find(|r| r.origin == sender)
        .cloned()
        .unwrap_or_else(|| Relationship {
            welcomed: true,
            ..Relationship::new(pairwise.did, sender.to_owned(), false)
        }))
}

/// Packs `message` from `from` to `to` — authcrypted, wrapped in a `forward`
/// for `to`'s mediator — and posts it where `to`'s service says.
pub async fn send(
    mediator: &Mediator,
    from: &Peer,
    to: &str,
    message: Message,
) -> Result<(), MessagingError> {
    send_with(mediator, from, to, message, Urgency::Normal).await
}

/// [`send`], with `urgency` on its `forward`: [`Urgency::Call`] has `to`'s
/// mediator ring the phone rather than send the ordinary wake-up.
pub async fn send_with(
    mediator: &Mediator,
    from: &Peer,
    to: &str,
    message: Message,
    urgency: Urgency,
) -> Result<(), MessagingError> {
    let packed = message
        .from(&from.did)
        .to([to])
        .pack_encrypted(
            to,
            Some(&from.did),
            None,
            mediator.resolver(),
            &from.secrets(),
            PackOptions {
                urgency,
                ..PackOptions::default()
            },
        )
        .await
        .map_err(|_| MessagingError::CounterpartyUnreachable)?;
    let uri = packed
        .service_uri
        .ok_or(MessagingError::CounterpartyUnreachable)?;
    mediator::post(&uri, packed.message).await
}

/// The invitation's `id`: the first 16 bytes of the card's hash, in hex.
fn invitation_id(card: &str) -> String {
    Sha256::digest(card.as_bytes())[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "did:peer:2.Vz6MkCard";

    #[test]
    fn the_wallets_own_invitation_reads_back() {
        let invitation = read(&link(CARD)).expect("an invitation");
        assert_eq!(invitation.from, CARD);
        assert_eq!(invitation.id, invitation_id(CARD));
        // And it is the same every time it is shown.
        assert_eq!(link(CARD), link(CARD));
    }

    #[test]
    fn a_mediators_invitation_is_not_a_contacts() {
        let mediator = json!({
            "type": INVITATION,
            "id": "x",
            "from": "did:web:mediator.example.com",
            "body": {"goal_code": REQUEST_MEDIATE},
        });
        let url = format!(
            "https://mediator.example.com/oob?_oob={}",
            b64::encode(mediator.to_string())
        );
        assert!(matches!(
            read(&url),
            Err(MessagingError::InvitationUnreadable)
        ));
        for input in ["", "hello", "almena://invite", "almena://invite?_oob=!!"] {
            assert!(read(input).is_err(), "{input}");
        }
    }

    #[test]
    fn a_name_is_a_short_stable_fingerprint() {
        assert_eq!(name(CARD), name(CARD));
        assert_ne!(name(CARD), name("did:peer:2.other"));
        assert_eq!(name(CARD).chars().count(), 9);
    }
}
