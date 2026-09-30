//! Calls (`https://almena.id/protocols/call/1.0`, `SPEC.md` §3): the
//! signalling that sets up a WebRTC call between the two wallets of a
//! relationship. The call itself — the `RTCPeerConnection`, the microphone and
//! the camera — lives in the interface; this side carries its messages over
//! the pairwise, where the keys are, and gets the TURN credentials every call
//! is relayed with from the wallet's own mediator.
//!
//! **Nothing about a call is kept.** Signals are handed to the interface as
//! they arrive (the `call-signal` event) and forgotten; an offer that arrives
//! after it expired was a call that is no longer ringing, and is dropped.

use almena_didcomm::Message;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::mediator::Mediator;
use super::peer::Peer;
use super::MessagingError;

pub const OFFER: &str = "https://almena.id/protocols/call/1.0/offer";
pub const ANSWER: &str = "https://almena.id/protocols/call/1.0/answer";
pub const HANGUP: &str = "https://almena.id/protocols/call/1.0/hangup";

const CREDENTIALS_REQUEST: &str = "https://almena.id/protocols/turn/1.0/credentials-request";
const CREDENTIALS: &str = "https://almena.id/protocols/turn/1.0/credentials";

/// The event the interface listens to: an [`Incoming`].
pub const SIGNAL: &str = "call-signal";

/// How long an offer rings before it expires, in seconds.
const OFFER_TTL: u64 = 60;
/// The largest SDP taken. A relayed offer with audio and video is a few
/// kilobytes.
const SDP_BYTES: usize = 64 * 1024;

const REASONS: [&str; 6] = [
    "ended",
    "cancelled",
    "unanswered",
    "declined",
    "busy",
    "failed",
];

/// What one side of a call says to the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Signal {
    Offer { media: Media, sdp: String },
    Answer { sdp: String },
    Hangup { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Media {
    Audio,
    Video,
}

/// A signal that arrived, as the interface gets it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Incoming {
    /// The conversation it came in.
    pub contact: String,
    /// The call's id: the offer's `id`, the others' `thid`.
    pub call: String,
    pub signal: Signal,
}

/// The message that says `signal` in the call `call` — for an offer, a new
/// call, whose id is the message's.
pub fn message(call: Option<&str>, signal: &Signal) -> Result<Message, MessagingError> {
    let message = match signal {
        Signal::Offer { media, sdp } => {
            let mut offer = Message::new(OFFER, json!({"media": media, "sdp": relayed(sdp)}));
            offer.expires_time = offer.created_time.map(|at| at + OFFER_TTL);
            return Ok(offer);
        }
        Signal::Answer { sdp } => Message::new(ANSWER, json!({"sdp": relayed(sdp)})),
        Signal::Hangup { reason } => Message::new(HANGUP, json!({"reason": reason})),
    };
    let call = call.ok_or(MessagingError::MessageInvalid)?;
    Ok(Message {
        thid: Some(call.to_owned()),
        ..message
    })
}

/// The call and signal a message carries, when it is a valid one and not
/// expired at `now`.
pub fn read(message: &Message, now: u64) -> Option<(String, Signal)> {
    let sdp = || {
        message.body["sdp"]
            .as_str()
            .filter(|sdp| !sdp.is_empty() && sdp.len() <= SDP_BYTES)
            .map(relayed)
    };
    match message.type_.as_str() {
        OFFER => {
            if message.is_expired_at(now) {
                return None;
            }
            let media = serde_json::from_value(message.body["media"].clone()).ok()?;
            Some((message.id.clone(), Signal::Offer { media, sdp: sdp()? }))
        }
        ANSWER => Some((message.thid.clone()?, Signal::Answer { sdp: sdp()? })),
        HANGUP => {
            let reason = message.body["reason"]
                .as_str()
                .filter(|reason| REASONS.contains(reason))
                .unwrap_or("ended")
                .to_owned();
            Some((message.thid.clone()?, Signal::Hangup { reason }))
        }
        _ => None,
    }
}

/// The SDP without any candidate but relayed ones (`SPEC.md` §3.1): a host or
/// server-reflexive candidate would be somebody's address.
fn relayed(sdp: &str) -> String {
    sdp.split_inclusive('\n')
        .filter(|line| !line.starts_with("a=candidate:") || line.contains(" typ relay"))
        .collect()
}

/// The TURN servers this wallet's mediator lets it relay a call through, as
/// `RTCIceServer`s.
///
/// # Errors
///
/// [`MessagingError::CallsUnavailable`] when the mediator does not offer TURN.
pub async fn ice_servers(mediator: &Mediator, inbox: &Peer) -> Result<Value, MessagingError> {
    let credentials = match mediator
        .request(inbox, Message::new(CREDENTIALS_REQUEST, json!({})))
        .await
    {
        Ok(credentials) => credentials,
        Err(MessagingError::MediatorRefused) => return Err(MessagingError::CallsUnavailable),
        Err(error) => return Err(error),
    };
    match &credentials.body["ice_servers"] {
        servers @ Value::Array(list) if credentials.type_ == CREDENTIALS && !list.is_empty() => {
            Ok(servers.clone())
        }
        _ => Err(MessagingError::CallsUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDP: &str = "v=0\r\na=candidate:1 1 udp 2122 192.168.1.2 5000 typ host\r\na=candidate:2 1 udp 16 203.0.113.9 49160 typ relay raddr 0.0.0.0 rport 0\r\na=candidate:3 1 udp 1686 198.51.100.4 6000 typ srflx raddr 192.168.1.2 rport 5000\r\na=end-of-candidates\r\n";

    #[test]
    fn only_relayed_candidates_travel() {
        let sdp = relayed(SDP);
        assert!(sdp.contains("typ relay"));
        assert!(!sdp.contains("typ host"));
        assert!(!sdp.contains("typ srflx"));
        assert!(sdp.starts_with("v=0\r\n") && sdp.ends_with("a=end-of-candidates\r\n"));
    }

    #[test]
    fn a_call_is_a_thread() {
        let offer = message(
            None,
            &Signal::Offer {
                media: Media::Video,
                sdp: SDP.into(),
            },
        )
        .unwrap();
        let at = offer.created_time.unwrap();
        assert_eq!(offer.expires_time, Some(at + OFFER_TTL));
        assert!(!offer.body["sdp"].as_str().unwrap().contains("typ host"));
        let (call, signal) = read(&offer, at).unwrap();
        assert_eq!(call, offer.id);
        assert!(matches!(
            signal,
            Signal::Offer {
                media: Media::Video,
                ..
            }
        ));

        let answer = message(Some(&call), &Signal::Answer { sdp: SDP.into() }).unwrap();
        assert_eq!(answer.thid.as_deref(), Some(call.as_str()));
        assert_eq!(read(&answer, at).unwrap().0, call);

        // Only an offer starts a call.
        assert!(message(None, &Signal::Answer { sdp: SDP.into() }).is_err());
    }

    #[test]
    fn an_expired_offer_no_longer_rings() {
        let offer = message(
            None,
            &Signal::Offer {
                media: Media::Audio,
                sdp: SDP.into(),
            },
        )
        .unwrap();
        let expiry = offer.expires_time.unwrap();
        assert!(read(&offer, expiry - 1).is_some());
        assert!(read(&offer, expiry).is_none());
    }

    #[test]
    fn malformed_signals_are_dropped_and_unknown_reasons_read_as_ended() {
        let mut offer = Message::new(OFFER, json!({"media": "hologram", "sdp": SDP}));
        assert!(read(&offer, 0).is_none());
        offer.body = json!({"media": "audio"});
        assert!(read(&offer, 0).is_none());
        let mut hangup = Message::new(HANGUP, json!({"reason": "bored"}));
        assert!(read(&hangup, 0).is_none(), "a hangup needs its call");
        hangup.thid = Some("call".into());
        assert_eq!(
            read(&hangup, 0).unwrap().1,
            Signal::Hangup {
                reason: "ended".into()
            }
        );
    }
}
