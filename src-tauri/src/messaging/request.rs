//! The second code: what the portal shows once the person has filled in
//! the form, read here for the wallet to send.
//!
//! Bare JSON, not a link. It carries everything the wallet needs to compose
//! the request and to show what it is about to send — which run it answers,
//! which issuer it goes to, what the credential is called, which version of
//! the template the answers are to, and the answers with their labels — and
//! nothing the wallet does with it happens on its own: the person sees the
//! summary and presses the button, or does not.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::MessagingError;

/// The platform's own protocol for asking an issuer for a credential: what
/// the wallet says back to the marketplace's invitation.
pub const ACCEPT_TYPE: &str = "https://almena.id/credential-request/1.0/accept";
pub const REQUEST_TYPE: &str = "https://almena.id/credential-request/1.0/request";

/// One answer of the form, with the label it was asked under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub value: String,
}

/// Which template, and which version of it, the answers are to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub slug: String,
    pub version: u64,
}

/// The code, read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// The run the request answers: the invitation's id, named as the
    /// parent thread of what is sent.
    #[serde(rename = "pthid")]
    pub acquisition: String,
    /// The issuer's DID, which the request goes to.
    #[serde(rename = "to")]
    pub issuer: String,
    /// What the issuer is called, for the screen.
    #[serde(rename = "issuer")]
    pub issuer_name: String,
    /// What the credential is called, for the screen.
    pub credential: String,
    pub template: Template,
    pub fields: Vec<Field>,
}

/// What the person saw and authorised, kept with the request as sent. The
/// body that travels carries the answers by key, because the issuer has
/// the template and knows its own labels; the person does not, and what
/// they read back later is what they read before pressing the button —
/// the credential by name and each answer under the label it was asked
/// with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub credential: String,
    pub fields: Vec<Field>,
}

impl Request {
    /// What goes to the issuer: the template and the answers by key. The
    /// labels stay here — the issuer has the template, and knows its own.
    pub fn body(&self) -> Value {
        let fields: serde_json::Map<String, Value> = self
            .fields
            .iter()
            .map(|field| (field.key.clone(), json!(field.value)))
            .collect();
        json!({
            "template": { "slug": self.template.slug, "version": self.template.version },
            "fields": fields,
        })
    }

    /// What stays with the receipt: the summary the person authorised.
    pub fn summary(&self) -> Summary {
        Summary {
            credential: self.credential.clone(),
            fields: self.fields.clone(),
        }
    }
}

pub fn read(input: &str) -> Result<Request, MessagingError> {
    let json: Value =
        serde_json::from_str(input.trim()).map_err(|_| MessagingError::RequestUnreadable)?;
    if json["type"].as_str() != Some(REQUEST_TYPE) {
        return Err(MessagingError::RequestUnreadable);
    }
    let request: Request =
        serde_json::from_value(json).map_err(|_| MessagingError::RequestUnreadable)?;
    let did_ok = request.issuer.starts_with("did:web:") || request.issuer.starts_with("did:key:");
    if !did_ok || request.acquisition.is_empty() || request.issuer.contains('#') {
        return Err(MessagingError::RequestUnreadable);
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> String {
        json!({
            "type": REQUEST_TYPE,
            "pthid": "a1b2c3",
            "to": "did:web:almena.id:i1",
            "issuer": "Degrees",
            "credential": "Degree certificate",
            "template": { "slug": "t1", "version": 3 },
            "fields": [
                { "key": "given_name", "label": "Given name", "value": "Ana" },
                { "key": "year", "label": "Year", "value": "2020" }
            ]
        })
        .to_string()
    }

    #[test]
    fn the_code_reads_as_a_request() {
        let request = read(&code()).unwrap();
        assert_eq!(request.acquisition, "a1b2c3");
        assert_eq!(request.issuer, "did:web:almena.id:i1");
        assert_eq!(request.issuer_name, "Degrees");
        assert_eq!(request.credential, "Degree certificate");
        assert_eq!(request.template.version, 3);
        assert_eq!(request.fields.len(), 2);
        assert_eq!(
            request.body(),
            json!({
                "template": { "slug": "t1", "version": 3 },
                "fields": { "given_name": "Ana", "year": "2020" }
            })
        );
        let summary = request.summary();
        assert_eq!(summary.credential, "Degree certificate");
        assert_eq!(summary.fields, request.fields);
    }

    #[test]
    fn what_is_not_a_request_is_refused() {
        assert!(read("hello").is_err());
        assert!(read(r#"{"type":"other"}"#).is_err());
        assert!(read(&code().replace("did:web:almena.id:i1", "did:example:1")).is_err());
        assert!(read(&code().replace("\"a1b2c3\"", "\"\"")).is_err());
    }
}
