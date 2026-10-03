//! What is said once a relationship is open: text (Basic Message 2.0) and the
//! name each side goes by (User Profile 1.0).
//!
//! **A name travels only over the pairwise.** It is sent to one counterparty at
//! a time, from the DID that counterparty knows, so it links nothing the
//! pairwise did not already. The one who invites sends theirs right after the
//! handshake and asks for the other's back (`send_back_yours`); a changed name
//! goes to every contact. Nothing else of a profile — picture, description —
//! is sent or read.

use almena_didcomm::Message;
use serde_json::{json, Value};

use super::contacts;
use super::mediator::Mediator;
use super::peer::Peer;
use super::MessagingError;

pub const TEXT: &str = "https://didcomm.org/basicmessage/2.0/message";
pub const PROFILE: &str = "https://didcomm.org/user-profile/1.0/profile";

/// The longest name kept, in characters: a name, not a biography.
pub const NAME_CHARS: usize = 64;
/// The longest text sent, in characters. Well inside what a mediator takes.
pub const TEXT_CHARS: usize = 4000;

/// A message of text, ready to send. Its `id` is what the conversation keeps.
pub fn text(content: &str) -> Message {
    Message::new(TEXT, json!({"content": content}))
}

/// The text of a Basic Message, when it has one.
pub fn read_text(message: &Message) -> Option<String> {
    message.body["content"]
        .as_str()
        .filter(|content| !content.trim().is_empty())
        .map(|content| content.chars().take(TEXT_CHARS).collect())
}

/// Sends this wallet's name — or says it has none — from `from` to `to`.
pub async fn send_profile(
    mediator: &Mediator,
    from: &Peer,
    to: &str,
    name: Option<&str>,
    send_back_yours: bool,
) -> Result<(), MessagingError> {
    let message = Message::new(
        PROFILE,
        json!({
            // An empty name removes the one sent before, as the protocol says.
            "profile": {"displayName": name.unwrap_or_default()},
            "send_back_yours": send_back_yours,
        }),
    );
    contacts::send(mediator, from, to, message).await
}

/// What a profile says about the name: `None` when it says nothing, `Some(None)`
/// when it removes it.
pub fn read_profile(message: &Message) -> Option<Option<String>> {
    match &message.body["profile"]["displayName"] {
        Value::String(name) => Some(clean(name)),
        Value::Null if message.body["profile"].get("displayName").is_some() => Some(None),
        _ => None,
    }
}

/// Whether a profile asks for this wallet's back.
pub fn wants_ours(message: &Message) -> bool {
    message.body["send_back_yours"] == true
}

/// A name as it is kept: trimmed, without control characters, at most
/// [`NAME_CHARS`] long; nothing when nothing is left.
pub fn clean(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(NAME_CHARS)
        .collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        assert_eq!(clean("  Ana \n"), Some("Ana".into()));
        assert_eq!(clean(" \t "), None);
        assert_eq!(clean(&"x".repeat(200)).map(|n| n.chars().count()), Some(64));
        assert_eq!(clean("A\u{7}na"), Some("Ana".into()));
    }

    #[test]
    fn a_profile_can_set_remove_or_leave_the_name() {
        let profile = |body| Message::new(PROFILE, body);
        assert_eq!(
            read_profile(&profile(json!({"profile": {"displayName": "Ana"}}))),
            Some(Some("Ana".into()))
        );
        assert_eq!(
            read_profile(&profile(json!({"profile": {"displayName": ""}}))),
            Some(None)
        );
        assert_eq!(
            read_profile(&profile(json!({"profile": {"displayName": null}}))),
            Some(None)
        );
        assert_eq!(read_profile(&profile(json!({"profile": {}}))), None);
    }

    #[test]
    fn empty_text_is_not_a_message() {
        assert_eq!(read_text(&text("hola")), Some("hola".into()));
        assert_eq!(read_text(&text("  ")), None);
        assert_eq!(read_text(&Message::new(TEXT, json!({"content": 3}))), None);
    }
}
