//! What somebody scanned or pasted, read as an invitation to a relationship.
//!
//! Three shapes are read, and all of them come to the same two things: the
//! DID the relationship would be with, and what it calls itself.
//!
//! - An out-of-band invitation URL, `https://…?_oob=<base64url JSON>`, which
//!   is how DIDComm v2 puts an invitation in a QR code or a link.
//! - The invitation's JSON itself.
//! - A bare DID, for an entity whose identifier somebody was simply handed.
//!
//! Nothing read here does anything by itself. It is the person who opens the
//! relationship, from a screen that shows them who it would be with.

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use super::MessagingError;

pub const OOB_INVITATION: &str = "https://didcomm.org/out-of-band/2.0/invitation";

/// The goal code the DIDComm community uses for "I will issue you a
/// credential": what the marketplace's invitation carries, and what makes
/// opening it the start of a request rather than just a relationship.
pub const ISSUE_GOAL: &str = "issue-vc";

/// Who an invitation is from, and what it says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub counterparty: String,
    pub label: Option<String>,
    /// The invitation's own id, which what answers it names as its parent
    /// thread. None for a bare DID, which is nobody's invitation in
    /// particular.
    pub id: Option<String>,
    /// What the sender says the relationship is for, as a goal code.
    pub goal_code: Option<String>,
}

impl Invitation {
    /// Whether opening this starts a request for a credential: the
    /// marketplace's invitation, which is answered rather than just opened.
    pub fn asks_for_credential(&self) -> bool {
        self.id.is_some() && self.goal_code.as_deref() == Some(ISSUE_GOAL)
    }
}

pub fn read(input: &str) -> Result<Invitation, MessagingError> {
    let input = input.trim();
    if input.starts_with("did:") {
        return did(input).map(|counterparty| Invitation {
            counterparty,
            label: None,
            id: None,
            goal_code: None,
        });
    }
    if let Ok(url) = url::Url::parse(input) {
        if let Some((_, encoded)) = url.query_pairs().find(|(name, _)| name == "_oob") {
            let bytes = URL_SAFE_NO_PAD
                .decode(encoded.as_bytes())
                .or_else(|_| URL_SAFE.decode(encoded.as_bytes()))
                .map_err(|_| MessagingError::InvitationUnreadable)?;
            let json: Value =
                serde_json::from_slice(&bytes).map_err(|_| MessagingError::InvitationUnreadable)?;
            return from_json(&json);
        }
    }
    if let Ok(json) = serde_json::from_str::<Value>(input) {
        return from_json(&json);
    }
    Err(MessagingError::InvitationUnreadable)
}

fn from_json(json: &Value) -> Result<Invitation, MessagingError> {
    if json["type"].as_str() != Some(OOB_INVITATION) {
        return Err(MessagingError::InvitationUnreadable);
    }
    let counterparty = did(json["from"]
        .as_str()
        .ok_or(MessagingError::InvitationUnreadable)?)?;
    // v2 invitations carry no label of their own; the `goal` is what the
    // sender wrote for the person to read, and a `label` is what most
    // implementations add anyway.
    let label = ["label", "goal"]
        .iter()
        .find_map(|name| json["body"][name].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let text = |value: &Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Ok(Invitation {
        counterparty,
        label,
        id: text(&json["id"]),
        goal_code: text(&json["body"]["goal_code"]),
    })
}

/// A DID of one of the two methods this wallet reaches, and nothing else in it.
fn did(value: &str) -> Result<String, MessagingError> {
    let clean = value.trim();
    let method_ok = clean.starts_with("did:web:") || clean.starts_with("did:key:");
    if !method_ok || clean.chars().any(char::is_whitespace) || clean.contains('#') {
        return Err(MessagingError::InvitationUnreadable);
    }
    Ok(clean.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_did_is_an_invitation_from_it() {
        let read = read(" did:web:almena.id:city-hall ").unwrap();
        assert_eq!(read.counterparty, "did:web:almena.id:city-hall");
        assert_eq!(read.label, None);
    }

    #[test]
    fn an_oob_url_carries_the_invitation() {
        let invitation = serde_json::json!({
            "type": OOB_INVITATION,
            "id": "1",
            "from": "did:web:almena.id:city-hall",
            "body": { "goal": "Residence certificate", "accept": ["didcomm/v2"] }
        });
        let encoded = URL_SAFE_NO_PAD.encode(invitation.to_string());
        let read = read(&format!("https://almena.id/invite?_oob={encoded}")).unwrap();
        assert_eq!(read.counterparty, "did:web:almena.id:city-hall");
        assert_eq!(read.label.as_deref(), Some("Residence certificate"));
        assert_eq!(read.id.as_deref(), Some("1"));
        assert_eq!(read.goal_code, None);
        assert!(!read.asks_for_credential());
    }

    #[test]
    fn the_marketplace_invitation_asks_for_a_credential() {
        let invitation = serde_json::json!({
            "type": OOB_INVITATION,
            "id": "a1b2c3",
            "from": "did:web:almena.id:i1",
            "body": { "goal_code": ISSUE_GOAL, "goal": "Degrees", "accept": ["didcomm/v2"] }
        });
        let asked = read(&invitation.to_string()).unwrap();
        assert_eq!(asked.id.as_deref(), Some("a1b2c3"));
        assert_eq!(asked.goal_code.as_deref(), Some(ISSUE_GOAL));
        assert!(asked.asks_for_credential());
        assert!(!read("did:web:almena.id:i1").unwrap().asks_for_credential());
    }

    #[test]
    fn what_is_not_an_invitation_is_refused() {
        assert!(read("hello").is_err());
        assert!(read("did:example:123").is_err());
        assert!(read("https://almena.id/?x=1").is_err());
        assert!(read(r#"{"type":"other","from":"did:web:a"}"#).is_err());
    }
}
