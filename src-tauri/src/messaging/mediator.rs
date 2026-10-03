//! Talking to a mediator: finding it from what somebody pasted, reading its DID
//! document, and the request/response the mediation protocols are made of.
//!
//! **Every request is authcrypted and asks for its answer on the same
//! connection** (`return_route: "all"`), because a wallet has no endpoint of its
//! own: the mediator answers `200` with the reply as the body. That is the
//! design in the mediator's SPEC.md (§6.1, §7.1), and it is the only way this
//! module talks to one.
//!
//! **HTTPS, except on this machine in a development build.** A mediator is
//! found through its `did:web`, whose document the method says is fetched over
//! HTTPS. A debug build also accepts plain HTTP for `localhost` and the loopback
//! addresses, so a mediator run with `task dev:memory` and
//! `ALMENA_PUBLIC_URL=http://localhost:8080` is reachable; a release build
//! never is.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use almena_didcomm::did::{
    web, ChainResolver, DidDocument, DidResolver, LocalResolver, StaticResolver,
};
use almena_didcomm::{b64, unpack, Message, PackOptions, PossessionProof};
use async_trait::async_trait;
use serde_json::{json, Value};
use url::Url;

use super::peer::Peer;
use super::MessagingError;

/// The media type of every envelope this module sends and expects back.
const ENCRYPTED: &str = "application/didcomm-encrypted+json";

/// Out-of-Band 2.0's invitation message type.
const INVITATION: &str = "https://didcomm.org/out-of-band/2.0/invitation";

/// The goal a mediator's invitation carries.
const REQUEST_MEDIATE: &str = "request-mediate";

const MEDIATE_REQUEST: &str = "https://didcomm.org/coordinate-mediation/3.0/mediate-request";
const RECIPIENT_UPDATE: &str = "https://didcomm.org/coordinate-mediation/3.0/recipient-update";
const DELIVERY_REQUEST: &str = "https://didcomm.org/messagepickup/3.0/delivery-request";
const MESSAGES_RECEIVED: &str = "https://didcomm.org/messagepickup/3.0/messages-received";

/// How many messages one `delivery-request` asks for; the mediator caps it at
/// 100.
const DELIVERY_LIMIT: u64 = 50;

/// How long a mediator gets to answer before it counts as unreachable.
const TIMEOUT: Duration = Duration::from_secs(20);

/// A mediator, as far as the wallet needs to know one before talking to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Its DID, a `did:web`.
    pub did: String,
    /// The invitation's `id`, when it came from one: the `pthid` of the
    /// `mediate-request` that answers it.
    pub invitation: Option<String>,
}

/// Reads what somebody pasted: the invitation URL a mediator shows
/// (`…/oob?_oob=…`), the mediator's address (`https://host`), or its DID.
///
/// # Errors
///
/// [`MessagingError::InvitationUnreadable`] for anything else, including an
/// invitation that is not a mediator's.
pub fn target(input: &str) -> Result<Target, MessagingError> {
    let input = input.trim();
    if input.starts_with("did:web:") {
        web::document_url(input).map_err(|_| MessagingError::InvitationUnreadable)?;
        return Ok(Target {
            did: input.to_owned(),
            invitation: None,
        });
    }

    let url = Url::parse(input).map_err(|_| MessagingError::InvitationUnreadable)?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err(MessagingError::InvitationUnreadable);
    }

    if let Some((_, encoded)) = url.query_pairs().find(|(name, _)| name == "_oob") {
        return invitation(&encoded);
    }

    // An address: the mediator's DID is the `did:web` of its origin.
    let origin = url.origin().ascii_serialization();
    let did = web::did_from_origin(&origin).map_err(|_| MessagingError::InvitationUnreadable)?;
    Ok(Target {
        did,
        invitation: None,
    })
}

/// The mediator an Out-of-Band invitation names.
fn invitation(encoded: &str) -> Result<Target, MessagingError> {
    // Base64url without padding is what the spec writes; a padded one is read
    // too rather than refused over an `=`.
    let bytes = b64::decode(encoded.trim_end_matches('='))
        .map_err(|_| MessagingError::InvitationUnreadable)?;
    let invitation: Value =
        serde_json::from_slice(&bytes).map_err(|_| MessagingError::InvitationUnreadable)?;

    if invitation["type"] != INVITATION {
        return Err(MessagingError::InvitationUnreadable);
    }
    // An invitation to something other than mediation — a contact's, say — is
    // not one to answer from here.
    if let Some(goal) = invitation["body"]["goal_code"].as_str() {
        if goal != REQUEST_MEDIATE {
            return Err(MessagingError::InvitationUnreadable);
        }
    }
    let did = invitation["from"]
        .as_str()
        .filter(|from| from.starts_with("did:web:"))
        .ok_or(MessagingError::InvitationUnreadable)?;

    Ok(Target {
        did: did.to_owned(),
        invitation: invitation["id"].as_str().map(str::to_owned),
    })
}

/// A mediator whose document has been read: who it is and where it listens.
pub struct Mediator {
    pub did: String,
    /// The HTTP(S) URI of its `DIDCommMessaging` service.
    endpoint: String,
    /// The WebSocket URI of its `DIDCommMessaging` service, for live delivery,
    /// when it has one this wallet may use.
    pub socket: Option<String>,
    resolver: ChainResolver,
}

impl Mediator {
    /// Fetches the mediator's DID document and picks the endpoint to post to.
    ///
    /// # Errors
    ///
    /// [`MessagingError::Insecure`] for a plain HTTP mediator that is not on
    /// this machine (or any, in a release build), and
    /// [`MessagingError::MediatorUnreachable`] when the document cannot be read
    /// or names no endpoint this wallet can use.
    pub async fn resolve(did: &str) -> Result<Self, MessagingError> {
        let document = fetch(did).await?;
        let endpoint = endpoint(&document)?;
        let socket = socket(&document);
        let resolver = ChainResolver::new(vec![
            Arc::new(StaticResolver::new([document])),
            Arc::new(LocalResolver::new()),
            Arc::new(WebResolver),
        ]);

        Ok(Self {
            did: did.to_owned(),
            endpoint,
            socket,
            resolver,
        })
    }

    /// Sends one message from the inbox and returns the mediator's answer.
    ///
    /// # Errors
    ///
    /// [`MessagingError::MediatorUnreachable`] when it does not answer or the
    /// answer cannot be opened, and [`MessagingError::MediatorRefused`] when the
    /// answer is a problem report.
    pub async fn request(&self, inbox: &Peer, message: Message) -> Result<Message, MessagingError> {
        let packed = self.pack(inbox, message).await?;
        let response = client()?
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
            .body(packed)
            .send()
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(MessagingError::MediatorUnreachable);
        }
        let body = response
            .text()
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;

        self.open(inbox, &body).await
    }

    /// Packs a message from the inbox to the mediator: authcrypted, with its
    /// answer asked for on the same connection.
    pub async fn pack(&self, inbox: &Peer, message: Message) -> Result<String, MessagingError> {
        message
            .from(&inbox.did)
            .to([self.did.as_str()])
            .header("return_route", json!("all"))
            .pack_encrypted(
                &self.did,
                Some(&inbox.did),
                None,
                &self.resolver,
                &inbox.secrets(),
                PackOptions::default(),
            )
            .await
            .map(|packed| packed.message)
            .map_err(|_| MessagingError::Keys)
    }

    /// Opens what the mediator sent the inbox.
    ///
    /// # Errors
    ///
    /// [`MessagingError::MediatorUnreachable`] when it does not open or the
    /// mediator did not authcrypt it, and [`MessagingError::MediatorRefused`]
    /// when it is a problem report.
    pub async fn open(&self, inbox: &Peer, envelope: &str) -> Result<Message, MessagingError> {
        let (reply, metadata) = unpack(envelope, &self.resolver, &inbox.secrets())
            .await
            .map_err(|_| MessagingError::MediatorUnreachable)?;
        // Only an answer the mediator itself authcrypted is an answer.
        if !metadata.authenticated || reply.from.as_deref() != Some(self.did.as_str()) {
            return Err(MessagingError::MediatorUnreachable);
        }
        if reply.type_.ends_with("/problem-report") {
            return Err(MessagingError::MediatorRefused);
        }
        Ok(reply)
    }

    /// Asks for mediation for the inbox and registers the inbox with it.
    ///
    /// **Harmless to repeat, and repeated on purpose.** The mediator grants the
    /// same mediation to the same inbox and answers `no_change` to a DID it
    /// already has; a mediator that forgot the wallet — its mediation expired,
    /// or it was reset — has it back before anything else is asked of it.
    pub async fn ensure(
        &self,
        inbox: &Peer,
        invitation: Option<String>,
    ) -> Result<(), MessagingError> {
        let mut request = Message::new(MEDIATE_REQUEST, json!({}));
        request.pthid = invitation;
        let grant = self.request(inbox, request).await?;
        let routes_here = grant.type_.ends_with("/mediate-grant")
            && grant.body["routing_did"]
                .as_array()
                .is_some_and(|dids| dids.iter().any(|did| did == self.did.as_str()));
        // Every DID's service names this mediator as its route; a grant that
        // routes through something else would leave them all pointing at the
        // wrong place.
        if !routes_here {
            return Err(MessagingError::MediatorRefused);
        }
        self.register(inbox, inbox).await
    }

    /// Resolves DIDs: this mediator's from what was fetched, `did:key` and
    /// `did:peer` locally, and any other `did:web` — another mediator — over
    /// the network.
    pub fn resolver(&self) -> &ChainResolver {
        &self.resolver
    }

    /// Adds or removes `peer` as a recipient of the inbox's mediation, and
    /// returns the mediator's result for it (`success`, `no_change`,
    /// `client_error`, …).
    ///
    /// A DID other than the inbox itself carries a possession proof: a JWT it
    /// signs, naming this mediator and the inbox — the Almena extension the
    /// mediator requires before it routes a DID to somebody (its
    /// SPEC.md §6.2, "Recipient possession proof").
    pub async fn recipient(
        &self,
        inbox: &Peer,
        peer: &Peer,
        action: &str,
    ) -> Result<String, MessagingError> {
        let mut update = json!({"recipient_did": peer.did, "action": action});
        if action == "add" && peer.did != inbox.did {
            let proof = PossessionProof::new(&peer.did, &self.did, &inbox.did)
                .pack(None, &self.resolver, &peer.secrets())
                .await
                .map_err(|_| MessagingError::Keys)?;
            update["proof"] = json!(proof);
        }

        let reply = self
            .request(
                inbox,
                Message::new(RECIPIENT_UPDATE, json!({"updates": [update]})),
            )
            .await?;
        let result = reply.body["updated"]
            .as_array()
            .and_then(|updated| {
                updated
                    .iter()
                    .find(|entry| entry["recipient_did"] == peer.did.as_str())
            })
            .and_then(|entry| entry["result"].as_str())
            .ok_or(MessagingError::MediatorUnreachable)?;

        Ok(result.to_owned())
    }

    /// Registers `peer` so that messages for it are queued for the inbox.
    pub async fn register(&self, inbox: &Peer, peer: &Peer) -> Result<(), MessagingError> {
        match self.recipient(inbox, peer, "add").await?.as_str() {
            "success" | "no_change" => Ok(()),
            _ => Err(MessagingError::MediatorRefused),
        }
    }

    /// The oldest messages waiting for the inbox's mediation, as `(queue id,
    /// envelope)` pairs. They stay queued until [`Self::acknowledge`].
    pub async fn deliveries(&self, inbox: &Peer) -> Result<Vec<(String, String)>, MessagingError> {
        let reply = self
            .request(
                inbox,
                Message::new(DELIVERY_REQUEST, json!({"limit": DELIVERY_LIMIT})),
            )
            .await?;
        Ok(delivered(reply))
    }

    /// Tells the mediator these messages arrived, which is what removes them
    /// from the queue.
    pub async fn acknowledge(&self, inbox: &Peer, ids: &[String]) -> Result<(), MessagingError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.request(
            inbox,
            Message::new(MESSAGES_RECEIVED, json!({"message_id_list": ids})),
        )
        .await
        .map(|_| ())
    }
}

/// What a `delivery` carries, as `(queue id, envelope)` pairs; nothing for any
/// other message — an empty queue is answered with a `status`.
pub fn delivered(reply: Message) -> Vec<(String, String)> {
    if !reply.type_.ends_with("/delivery") {
        return Vec::new();
    }
    reply
        .attachments
        .unwrap_or_default()
        .into_iter()
        .filter_map(|attachment| {
            let id = attachment.id?;
            let bytes = b64::decode(attachment.data.base64.as_deref()?).ok()?;
            Some((id, String::from_utf8(bytes).ok()?))
        })
        .collect()
}

/// Posts a packed message to the transport URI its recipient's service names —
/// which, for anybody reached through a mediator, is that mediator.
///
/// # Errors
///
/// [`MessagingError::Insecure`] for plain HTTP off this machine, and
/// [`MessagingError::CounterpartyUnreachable`] when nobody takes it.
pub async fn post(uri: &str, message: String) -> Result<(), MessagingError> {
    let uri = transport(uri)?;
    let response = client()?
        .post(uri)
        .header(reqwest::header::CONTENT_TYPE, ENCRYPTED)
        .body(message)
        .send()
        .await
        .map_err(|_| MessagingError::CounterpartyUnreachable)?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(MessagingError::CounterpartyUnreachable)
    }
}

/// Fetches a `did:web` document and checks it is the one asked for.
async fn fetch(did: &str) -> Result<DidDocument, MessagingError> {
    let url = web::document_url(did).map_err(|_| MessagingError::InvitationUnreadable)?;
    let url = transport(&url)?;

    let response = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?;
    if !response.status().is_success() {
        return Err(MessagingError::MediatorUnreachable);
    }
    let json: Value = response
        .json()
        .await
        .map_err(|_| MessagingError::MediatorUnreachable)?;
    let document =
        DidDocument::from_json(&json).map_err(|_| MessagingError::MediatorUnreachable)?;
    // A document served for another DID is not that DID's, whatever it says
    // about itself.
    if document.id != did {
        return Err(MessagingError::MediatorUnreachable);
    }
    Ok(document)
}

/// Resolves `did:web` over HTTPS (and loopback HTTP in a debug build): how a
/// counterparty's mediator is found when it is not this wallet's own.
struct WebResolver;

#[async_trait]
impl DidResolver for WebResolver {
    async fn resolve(&self, did: &str) -> almena_didcomm::Result<DidDocument> {
        if !did.starts_with("did:web:") {
            return Err(almena_didcomm::Error::DidNotFound(did.to_owned()));
        }
        fetch(did)
            .await
            .map_err(|_| almena_didcomm::Error::DidNotFound(did.to_owned()))
    }
}

/// The first HTTP(S) endpoint of the document's `DIDCommMessaging` service.
fn endpoint(document: &DidDocument) -> Result<String, MessagingError> {
    let mut insecure = false;
    for service in document.didcomm_services() {
        for endpoint in service.didcomm_endpoints().unwrap_or_default() {
            if !endpoint.uri.starts_with("http") {
                continue;
            }
            match transport(&endpoint.uri) {
                Ok(uri) => return Ok(uri),
                Err(MessagingError::Insecure) => insecure = true,
                Err(_) => {}
            }
        }
    }
    Err(if insecure {
        MessagingError::Insecure
    } else {
        MessagingError::MediatorUnreachable
    })
}

/// The first WebSocket endpoint of the document's `DIDCommMessaging` service
/// that this build may use: `wss`, or `ws` on this machine in a debug build.
fn socket(document: &DidDocument) -> Option<String> {
    document
        .didcomm_services()
        .flat_map(|service| service.didcomm_endpoints().unwrap_or_default())
        .find_map(|endpoint| {
            let url = Url::parse(&endpoint.uri).ok()?;
            match url.scheme() {
                "wss" => Some(endpoint.uri),
                "ws" if cfg!(debug_assertions) && is_loopback(&url) => Some(endpoint.uri),
                _ => None,
            }
        })
}

/// The URL to actually use: HTTPS as it is, and plain HTTP only for this
/// machine in a debug build. A `did:web` document URL for a loopback host is
/// turned into HTTP there, because that is what a local mediator serves.
pub(crate) fn transport(url: &str) -> Result<String, MessagingError> {
    let parsed = Url::parse(url).map_err(|_| MessagingError::MediatorUnreachable)?;
    let local = cfg!(debug_assertions) && is_loopback(&parsed);

    match parsed.scheme() {
        "https" if local => Ok(url.replacen("https://", "http://", 1)),
        "https" => Ok(url.to_owned()),
        "http" if local => Ok(url.to_owned()),
        "http" => Err(MessagingError::Insecure),
        _ => Err(MessagingError::MediatorUnreachable),
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name == "localhost",
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

/// One HTTP client for the process: Rustls with `ring` and the Mozilla roots,
/// no redirects (a mediator that moves is a mediator to look up again, not one
/// to follow), and a timeout.
pub(crate) fn client() -> Result<&'static reqwest::Client, MessagingError> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();

    CLIENT
        .get_or_init(|| {
            let builder = reqwest::Client::builder()
                .tls_backend_preconfigured(Arc::unwrap_or_clone(tls().ok()?))
                .redirect(reqwest::redirect::Policy::none())
                .timeout(TIMEOUT);
            // Each `#[tokio::test]` has a runtime of its own, and a pooled
            // connection dies with the runtime that opened it; the application
            // has one runtime for its whole life and keeps the pool.
            #[cfg(test)]
            let builder = builder.pool_max_idle_per_host(0);
            builder.build().ok()
        })
        .as_ref()
        .ok_or(MessagingError::MediatorUnreachable)
}

/// The TLS every connection to a mediator (and a registry) is made with:
/// Rustls with `ring` and the Mozilla roots, the same for HTTPS and for the
/// WebSocket. A debug build on a computer also trusts the system's roots, so a
/// development service behind a locally trusted CA can be reached; a release
/// build never does.
pub fn tls() -> Result<Arc<rustls::ClientConfig>, MessagingError> {
    static TLS: OnceLock<Option<Arc<rustls::ClientConfig>>> = OnceLock::new();

    TLS.get_or_init(|| {
        #[allow(unused_mut)]
        let mut roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        #[cfg(all(debug_assertions, not(any(target_os = "android", target_os = "ios"))))]
        for cert in rustls_native_certs::load_native_certs().certs {
            // One the store cannot parse is left out, not fatal.
            let _ = roots.add(cert);
        }
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .ok()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        Some(Arc::new(config))
    })
    .clone()
    .ok_or(MessagingError::MediatorUnreachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DID: &str = "did:web:mediator.example.com";

    fn invitation_url(invitation: &Value) -> String {
        format!(
            "https://mediator.example.com/oob?_oob={}",
            b64::encode(invitation.to_string().as_bytes())
        )
    }

    #[test]
    fn a_mediator_invitation_names_the_mediator_and_its_thread() {
        let url = invitation_url(&json!({
            "type": INVITATION,
            "id": "invitation-1",
            "from": DID,
            "body": {"goal_code": REQUEST_MEDIATE, "accept": ["didcomm/v2"]},
        }));
        assert_eq!(
            target(&url).expect("an invitation"),
            Target {
                did: DID.to_owned(),
                invitation: Some("invitation-1".to_owned()),
            }
        );
    }

    #[test]
    fn an_address_or_a_did_names_the_mediator_too() {
        for input in [
            "https://mediator.example.com",
            "https://mediator.example.com/",
            "  https://Mediator.Example.com/some/page ",
            DID,
        ] {
            assert_eq!(target(input).expect("a mediator").did, DID, "{input}");
        }
        assert_eq!(
            target("http://localhost:8080").expect("a mediator").did,
            "did:web:localhost%3A8080"
        );
    }

    #[test]
    fn what_is_not_a_mediator_invitation_is_refused() {
        let contact = invitation_url(&json!({
            "type": INVITATION,
            "id": "x",
            "from": DID,
            "body": {"goal_code": "connect"},
        }));
        let not_web = invitation_url(&json!({
            "type": INVITATION,
            "id": "x",
            "from": "did:peer:2.Ez6LS",
            "body": {"goal_code": REQUEST_MEDIATE},
        }));
        for input in [
            "",
            "mediator",
            "ftp://mediator.example.com",
            "https://mediator.example.com/oob?_oob=not-base64!",
            contact.as_str(),
            not_web.as_str(),
        ] {
            assert!(
                matches!(target(input), Err(MessagingError::InvitationUnreadable)),
                "{input}"
            );
        }
    }

    #[test]
    fn plain_http_is_only_for_this_machine_in_a_debug_build() {
        assert_eq!(
            transport("https://mediator.example.com/didcomm").expect("https"),
            "https://mediator.example.com/didcomm"
        );
        assert!(matches!(
            transport("http://mediator.example.com/didcomm"),
            Err(MessagingError::Insecure)
        ));
        for local in [
            "http://localhost:8080/didcomm",
            "http://127.0.0.1:8080/didcomm",
            "http://[::1]:8080/didcomm",
        ] {
            assert_eq!(transport(local).is_ok(), cfg!(debug_assertions), "{local}");
        }
        if cfg!(debug_assertions) {
            assert_eq!(
                transport("https://localhost:8080/.well-known/did.json").expect("local"),
                "http://localhost:8080/.well-known/did.json"
            );
        }
    }
}
