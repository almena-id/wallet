//! Talking to a mediator, as one pairwise: opening a mailbox there, asking
//! what is waiting in it, taking it, and confirming what was taken.
//!
//! Every request is one round trip. It is encrypted for the mediator and
//! authenticated as the pairwise — authcrypt, which is what the mediator
//! requires of everything but a forward — and it carries `return_route: all`,
//! so the answer comes back in the same HTTP response, encrypted for the
//! pairwise in turn. A mailbox is exactly what that pairwise asked for and
//! nothing else, so there is no recipient to register: the mediator receives
//! for the DID that opened the mailbox — see the mediator's own README.
//!
//! **Each relationship talks for itself.** A pairwise never names another in
//! anything it sends, and two of them never share a request: what the
//! mediator can see is that a mailbox was emptied, not whose else was.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use didcomm::{AttachmentData, Message, PackEncryptedOptions, UnpackOptions};
use serde_json::{json, Value};

use super::did::Resolver;
use super::pairwise::Pairwise;
use super::MessagingError;

pub const ENCRYPTED: &str = "application/didcomm-encrypted+json";

const MEDIATE_REQUEST: &str = "https://didcomm.org/coordinate-mediation/3.0/mediate-request";
const MEDIATE_GRANT: &str = "https://didcomm.org/coordinate-mediation/3.0/mediate-grant";
const RECIPIENT_UPDATE: &str = "https://didcomm.org/coordinate-mediation/3.0/recipient-update";
const RECIPIENT_UPDATE_RESPONSE: &str =
    "https://didcomm.org/coordinate-mediation/3.0/recipient-update-response";
const DELIVERY_REQUEST: &str = "https://didcomm.org/messagepickup/3.0/delivery-request";
const DELIVERY: &str = "https://didcomm.org/messagepickup/3.0/delivery";
const STATUS: &str = "https://didcomm.org/messagepickup/3.0/status";
const MESSAGES_RECEIVED: &str = "https://didcomm.org/messagepickup/3.0/messages-received";
const PROBLEM_REPORT: &str = "https://didcomm.org/report-problem/2.0/problem-report";

/// The most one delivery is asked for. The mediator caps it at the same.
const DELIVERY_LIMIT: u32 = 100;

/// A pairwise's mailbox at a mediator.
pub struct Mailbox<'a> {
    resolver: &'a Resolver,
    pairwise: &'a Pairwise,
    mediator: &'a str,
}

/// A message taken out of the mailbox, still to be confirmed.
pub struct Delivered {
    /// The attachment id, which is what the mediator deletes by.
    pub id: String,
    pub message: Message,
    /// Whether the envelope proved the `from` it names: authcrypt, as
    /// opposed to a sender who only wrote a name in. What is kept is kept
    /// either way; what a relationship is opened with is only the former.
    pub authenticated: bool,
}

impl<'a> Mailbox<'a> {
    pub fn new(resolver: &'a Resolver, pairwise: &'a Pairwise, mediator: &'a str) -> Self {
        Self {
            resolver,
            pairwise,
            mediator,
        }
    }

    /// Asks the mediator for a mailbox. Asking twice is fine and answers the
    /// same: the mailbox exists.
    pub async fn open(&self) -> Result<(), MessagingError> {
        let grant = self.exchange(MEDIATE_REQUEST, json!({})).await?;
        if grant.type_ != MEDIATE_GRANT {
            return Err(MessagingError::MediatorRefused);
        }
        Ok(())
    }

    /// Gives the mailbox back, with whatever is still in it. The protocol
    /// has no word for this; removing the one DID the mailbox receives for —
    /// its own — is what the mediator takes it to mean. What is left behind
    /// otherwise is a mailbox nobody will ever empty, which the mediator
    /// would only get round to by its lease.
    pub async fn close(&self) -> Result<(), MessagingError> {
        let response = self
            .exchange(
                RECIPIENT_UPDATE,
                json!({ "updates": [{ "recipient_did": self.pairwise.did, "action": "remove" }] }),
            )
            .await?;
        if response.type_ != RECIPIENT_UPDATE_RESPONSE {
            return Err(MessagingError::MediatorRefused);
        }
        Ok(())
    }

    /// Takes what is waiting, opened as the pairwise. What cannot be opened is
    /// still confirmed by the caller, because leaving it there would only
    /// have it delivered again.
    pub async fn take(&self) -> Result<Vec<Delivered>, MessagingError> {
        let reply = self
            .exchange(DELIVERY_REQUEST, json!({ "limit": DELIVERY_LIMIT }))
            .await?;
        match reply.type_.as_str() {
            DELIVERY => {}
            // Nothing waiting is answered with a status, as the protocol says.
            STATUS => return Ok(Vec::new()),
            _ => return Err(MessagingError::MediatorRefused),
        }
        let mut delivered = Vec::new();
        for attachment in reply.attachments.unwrap_or_default() {
            let Some(id) = attachment.id else {
                continue;
            };
            let packed = match attachment.data {
                AttachmentData::Base64 { value } => URL_SAFE_NO_PAD
                    .decode(value.base64.trim_end_matches('='))
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok()),
                AttachmentData::Json { value } => Some(value.json.to_string()),
                AttachmentData::Links { .. } => None,
            };
            let Some(packed) = packed else {
                log::info!(target: crate::develop::TARGET, "a delivery could not be read");
                continue;
            };
            match self.unpack(&packed).await {
                Ok((message, authenticated)) => delivered.push(Delivered {
                    id,
                    message,
                    authenticated,
                }),
                Err(_) => {
                    log::info!(target: crate::develop::TARGET, "a delivery could not be opened");
                }
            }
        }
        Ok(delivered)
    }

    /// Tells the mediator what was taken, which is the one thing that deletes.
    pub async fn confirm(&self, ids: &[String]) -> Result<(), MessagingError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.exchange(MESSAGES_RECEIVED, json!({ "message_id_list": ids }))
            .await
            .map(|_| ())
    }

    async fn exchange(&self, type_: &str, body: Value) -> Result<Message, MessagingError> {
        let endpoint = self
            .resolver
            .endpoint(self.mediator)
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?
            .ok_or(MessagingError::MediatorUnreachable)?;

        let request = Message::build(uuid::Uuid::new_v4().to_string(), type_.to_owned(), body)
            .from(self.pairwise.did.clone())
            .to(self.mediator.to_owned())
            .header("return_route".into(), json!("all"))
            .finalize();
        let (packed, _) = request
            .pack_encrypted(
                self.mediator,
                Some(&self.pairwise.did),
                None,
                self.resolver,
                &self.pairwise.secrets(),
                &PackEncryptedOptions {
                    forward: false,
                    ..Default::default()
                },
            )
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;

        let response = self
            .resolver
            .http()
            .post(&endpoint)
            .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
            .body(packed)
            .send()
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;
        if !response.status().is_success() {
            return Err(MessagingError::MediatorRefused);
        }
        let text = response
            .text()
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;
        let (reply, _) = self.unpack(&text).await?;
        if reply.type_ == PROBLEM_REPORT {
            log::info!(
                target: crate::develop::TARGET,
                "the mediator answered with a problem: {}",
                reply.body["code"].as_str().unwrap_or("?")
            );
            return Err(MessagingError::MediatorRefused);
        }
        Ok(reply)
    }

    async fn unpack(&self, packed: &str) -> Result<(Message, bool), MessagingError> {
        Message::unpack(
            packed,
            self.resolver,
            &self.pairwise.secrets(),
            &UnpackOptions::default(),
        )
        .await
        .map(|(message, metadata)| (message, metadata.authenticated))
        .map_err(|_| MessagingError::MediatorRefused)
    }
}

/// Sends `message` from `speaker` to `to`, the way `to`'s document says it
/// is reached: an entity of this platform names the mediator as its
/// endpoint, so the message is encrypted for the entity, wrapped in a
/// forward for the mediator, and posted there — the library does the
/// wrapping from the document, and says where the result goes. Nothing is
/// waited for beyond the mediator's acceptance: what the entity makes of it
/// comes back, if it does, through the speaker's own mailbox.
pub async fn send(
    resolver: &Resolver,
    speaker: &Pairwise,
    to: &str,
    message: Message,
) -> Result<(), MessagingError> {
    let (packed, metadata) = message
        .pack_encrypted(
            to,
            Some(&speaker.did),
            None,
            resolver,
            &speaker.secrets(),
            &PackEncryptedOptions {
                forward: true,
                ..Default::default()
            },
        )
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?;
    let endpoint = match metadata.messaging_service {
        Some(service) => service.service_endpoint,
        // No forward was needed: the recipient is reached directly, at the
        // endpoint its own document names. The library says nothing then.
        None => resolver
            .endpoint(to)
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?
            .ok_or(MessagingError::MediatorUnreachable)?,
    };
    let response = resolver
        .http()
        .post(&endpoint)
        .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
        .body(packed)
        .send()
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?;
    if !response.status().is_success() {
        return Err(MessagingError::MediatorRefused);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! The whole conversation, against a mediator that is actually running.
    //!
    //! Ignored unless `ALMENA_MEDIATOR` names one — `did:web:localhost%3A8100`
    //! for the mediator repository's `cargo run` with its DID and endpoint
    //! pointed at loopback — because a unit test that needs a service is not
    //! a unit test. Run with `cargo test -- --ignored messaging::mediator`.

    use didcomm::algorithms::AnonCryptAlg;
    use didcomm::protocols::routing::wrap_in_forward;
    use serde_json::json;

    use super::*;

    fn mediator_under_test() -> Option<String> {
        std::env::var("ALMENA_MEDIATOR")
            .ok()
            .filter(|s| !s.is_empty())
    }

    #[tokio::test]
    #[ignore = "needs a running mediator, named by ALMENA_MEDIATOR"]
    async fn a_pairwise_opens_a_mailbox_and_empties_it() {
        let Some(mediator) = mediator_under_test() else {
            return;
        };
        let resolver = Resolver::new();
        // Two wallets: one that opens the mailbox, one that writes to it. The
        // second is a pairwise too, because that is what a did:key with keys
        // is; its counterparty is irrelevant.
        let holder_seed = [11u8; 64];
        let sender_seed = [22u8; 64];
        let pairwise = Pairwise::derive(&holder_seed, "did:web:example.test:issuer");
        let sender = Pairwise::derive(&sender_seed, &pairwise.did);
        let mailbox = Mailbox::new(&resolver, &pairwise, &mediator);

        mailbox.open().await.expect("the mailbox opens");
        // Starting clean, whatever an earlier run left behind.
        let stale = mailbox.take().await.expect("the mailbox answers");
        let stale_ids: Vec<String> = stale.iter().map(|d| d.id.clone()).collect();
        mailbox.confirm(&stale_ids).await.expect("confirms");

        // What an issuer does: encrypt for the pairwise, wrap in a forward
        // for the mediator, post it.
        let inner = Message::build(
            uuid::Uuid::new_v4().to_string(),
            "https://example.test/hello/1.0/hello".into(),
            json!({ "text": "hello from the test" }),
        )
        .from(sender.did.clone())
        .to(pairwise.did.clone())
        .created_time(1_700_000_000)
        .finalize();
        let (packed, _) = inner
            .pack_encrypted(
                &pairwise.did,
                Some(&sender.did),
                None,
                &resolver,
                &sender.secrets(),
                &PackEncryptedOptions {
                    forward: false,
                    ..Default::default()
                },
            )
            .await
            .expect("packs for the pairwise");
        let wrapped = wrap_in_forward(
            &packed,
            None,
            &pairwise.did,
            &vec![mediator.clone()],
            &AnonCryptAlg::default(),
            &resolver,
        )
        .await
        .expect("wraps in a forward");
        let endpoint = resolver
            .endpoint(&mediator)
            .await
            .expect("resolves")
            .expect("has an endpoint");
        let status = resolver
            .http()
            .post(&endpoint)
            .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
            .body(wrapped)
            .send()
            .await
            .expect("posts")
            .status();
        assert_eq!(status, reqwest::StatusCode::ACCEPTED);

        // Taken, opened as the pairwise, and confirmed — after which the
        // mailbox is empty again.
        let delivered = mailbox.take().await.expect("takes");
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0].message.id, inner.id);
        assert_eq!(delivered[0].message.body["text"], "hello from the test");
        assert_eq!(
            delivered[0].message.from.as_deref(),
            Some(sender.did.as_str())
        );
        assert_eq!(delivered[0].message.created_time, Some(1_700_000_000));

        let ids: Vec<String> = delivered.iter().map(|d| d.id.clone()).collect();
        mailbox.confirm(&ids).await.expect("confirms");
        assert!(mailbox.take().await.expect("takes").is_empty());
    }

    #[tokio::test]
    #[ignore = "needs a running mediator, named by ALMENA_MEDIATOR"]
    async fn an_invitation_key_is_written_to_and_says_who_by() {
        let Some(mediator) = mediator_under_test() else {
            return;
        };
        let resolver = Resolver::new();
        // The wallet shows an invitation; an entity — a did:key here, since
        // any DID with keys will do for the envelope — scans it and writes.
        let holder_seed = [33u8; 64];
        let invite = super::super::invite::Invite::draw(&holder_seed, &mediator).unwrap();
        let key = invite.key(&holder_seed).unwrap();
        let entity = Pairwise::derive(&[44u8; 64], &invite.did);
        let mailbox = Mailbox::new(&resolver, &key, &mediator);
        mailbox
            .open()
            .await
            .expect("the invitation key's mailbox opens");

        let inner = Message::build(
            uuid::Uuid::new_v4().to_string(),
            "https://example.test/hello/1.0/hello".into(),
            json!({ "text": "I scanned your code" }),
        )
        .from(entity.did.clone())
        .to(invite.did.clone())
        .thid(invite.id.clone())
        .finalize();
        let (packed, _) = inner
            .pack_encrypted(
                &invite.did,
                Some(&entity.did),
                None,
                &resolver,
                &entity.secrets(),
                &PackEncryptedOptions {
                    forward: false,
                    ..Default::default()
                },
            )
            .await
            .expect("packs for the invitation key");
        let wrapped = wrap_in_forward(
            &packed,
            None,
            &invite.did,
            &vec![mediator.clone()],
            &AnonCryptAlg::default(),
            &resolver,
        )
        .await
        .expect("wraps in a forward");
        let endpoint = resolver.endpoint(&mediator).await.unwrap().unwrap();
        let status = resolver
            .http()
            .post(&endpoint)
            .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
            .body(wrapped)
            .send()
            .await
            .expect("posts")
            .status();
        assert_eq!(status, reqwest::StatusCode::ACCEPTED);

        // Taken as the invitation key, and the envelope says who wrote —
        // which is what the wallet derives the pairwise from.
        let delivered = mailbox.take().await.expect("takes");
        assert_eq!(delivered.len(), 1);
        assert!(delivered[0].authenticated);
        assert_eq!(
            delivered[0].message.from.as_deref(),
            Some(entity.did.as_str())
        );
        let ids: Vec<String> = delivered.iter().map(|d| d.id.clone()).collect();
        mailbox.confirm(&ids).await.expect("confirms");

        // The pairwise answers, from itself and carrying the rotation. The
        // entity of this test has no endpoint to be reached at, so the ping
        // goes to the mediator, which answers a trust ping and is the one
        // party here whose document says where it is written to.
        let pairwise = Pairwise::derive(&holder_seed, &entity.did);
        let (from_prior, _) = didcomm::FromPrior::build(invite.did.clone(), pairwise.did.clone())
            .finalize()
            .pack(
                Some(&format!("{}#key-1", invite.did)),
                &resolver,
                &key.secrets(),
            )
            .await
            .expect("the invitation key signs the rotation");
        let ping = Message::build(
            uuid::Uuid::new_v4().to_string(),
            super::super::TRUST_PING.to_owned(),
            json!({ "response_requested": false }),
        )
        .from(pairwise.did.clone())
        .to(mediator.clone())
        .from_prior(from_prior)
        .finalize();
        send(&resolver, &pairwise, &mediator, ping)
            .await
            .expect("the ping is accepted");

        // Spent, so given back: the mediator forgets the key, and asking it
        // for the mailbox's contents is asking as a stranger.
        mailbox.close().await.expect("the mailbox closes");
        assert!(mailbox.take().await.is_err());
    }
}
