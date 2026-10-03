//! Notices from an issuer this wallet applied to (`../api`, `notices.py`):
//! how an application stands — accepted, rejected, issued — written to the
//! pairwise this wallet named when it paired with the issuer (`registry.rs`,
//! `issuer_channel`). They join the conversation with the issuer; an issued
//! one says where to ask for the credential (`collect`).

use std::collections::BTreeMap;

use almena_didcomm::Message;
use serde::{Deserialize, Serialize};
use url::Url;

/// `https://almena.id/protocols/application/1.0/status`.
pub const STATUS: &str = "https://almena.id/protocols/application/1.0/status";

/// What one says, as the conversation keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    /// The application's id at the issuer.
    pub application: String,
    /// `accepted`, `rejected` or `issued`.
    pub status: String,
    /// The credential applied for: its id and its name by language.
    pub credential_type: String,
    #[serde(default)]
    pub credential_name: BTreeMap<String, String>,
    /// The issuer's name, as it gives it.
    pub issuer: String,
    /// The issuer's note on its decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Issued: where this wallet asks to receive it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collect: Option<String>,
}

const STATUSES: [&str; 3] = ["accepted", "rejected", "issued"];
const SHORT: usize = 200;
const NOTE: usize = 2000;

fn text(value: &serde_json::Value, limit: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty() && text.chars().count() <= limit).then(|| text.to_owned())
}

/// The notice `message` carries, if it is one and holds together.
pub fn read(message: &Message) -> Option<Notice> {
    if message.type_ != STATUS {
        return None;
    }
    let body = &message.body;
    let status = text(&body["status"], SHORT)?;
    if !STATUSES.contains(&status.as_str()) {
        return None;
    }
    let collect = match body.get("collect") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            // HTTPS, or plain HTTP for this machine in a debug build.
            let url = Url::parse(&text(value, 2048)?).ok()?;
            super::mediator::transport(url.as_str()).ok()?;
            Some(url.to_string())
        }
    };
    let credential_name = body["credential_name"]
        .as_object()
        .map(|names| {
            names
                .iter()
                .filter_map(|(language, name)| {
                    Some((language.chars().take(16).collect(), text(name, SHORT)?))
                })
                .take(16)
                .collect()
        })
        .unwrap_or_default();
    Some(Notice {
        application: text(&body["application"], 64)?,
        status,
        credential_type: text(&body["credential_type"], 64)?,
        credential_name,
        issuer: text(&body["issuer"], SHORT)?,
        note: body.get("note").and_then(|note| text(note, NOTE)),
        collect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(body: serde_json::Value) -> Message {
        Message::new(STATUS, body)
    }

    #[test]
    fn a_notice_is_read_and_checked() {
        let read = read(&message(json!({
            "application": "a1",
            "status": "issued",
            "credential_type": "membership",
            "credential_name": {"en": "Membership", "es": "Afiliación"},
            "issuer": "Club",
            "collect": "https://api.almena.id/api/v1/applications/a1/collect",
        })))
        .unwrap();
        assert_eq!(read.status, "issued");
        assert_eq!(read.credential_name["es"], "Afiliación");
        assert_eq!(
            read.collect.as_deref(),
            Some("https://api.almena.id/api/v1/applications/a1/collect")
        );
        for body in [
            json!({"application": "a1", "status": "maybe", "credential_type": "x", "issuer": "C"}),
            json!({"application": "a1", "status": "issued", "credential_type": "x"}),
            json!({"application": "a1", "status": "issued", "credential_type": "x", "issuer": "C",
                   "collect": "http://api.example.org/collect"}),
        ] {
            assert!(super::read(&message(body)).is_none());
        }
        assert!(super::read(&Message::new(
            "https://didcomm.org/basicmessage/2.0/message",
            json!({})
        ))
        .is_none());
    }
}
