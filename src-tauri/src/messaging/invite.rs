//! The invitation this wallet shows: a code for whoever is in front of it.
//!
//! **The identity is never what is shown.** The root `did:key` is the one
//! identifier every pairwise descends from, and a code that carries it hands
//! whoever scans it the thing the pairwise model exists to withhold. What the
//! code carries instead is an **invitation key**: a key of its own, made for
//! this one showing, that speaks for the wallet until it knows who scanned it.
//!
//! It cannot be a pairwise, because a pairwise is derived from the
//! counterparty — see [`super::pairwise`] — and at the moment the code is
//! shown there is none. So the key is walked down from the seed by a **salt
//! drawn at random**, `m/3'/…`, and the salt is what the book keeps: the key
//! is never written down, only what it takes to walk to it again.
//!
//! It is a `did:peer:2` rather than a `did:key`, for one reason: whoever
//! scans it has to know where to write, and a `did:key` cannot say. The
//! spec keeps `did:key` for pairwise and has the mediator's address travel
//! in the invitation; for an invitation the wallet itself hands out, the
//! identifier is the invitation, and `did:peer:2` is the form that lets an
//! identifier carry a service. The pairwise that follows is a `did:key`, as
//! the spec says.
//!
//! **One invitation lives at a time.** Showing a new code replaces the last
//! one: the previous key's mailbox is left behind, and a message that
//! arrives in it afterwards is nobody's. When somebody does write to the
//! invitation, the wallet learns who from the envelope, derives the pairwise
//! for them — the one it already had, if it had one — and answers from that
//! pairwise with a `from_prior` the invitation key signed, which is how
//! DIDComm says "this identifier is now that one". The invitation is spent
//! by that.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zeroize::Zeroizing;

use super::invitation::OOB_INVITATION;
use super::pairwise::{self, Named, Pairwise};
use crate::identity::keys;

/// Salt under the hash that turns the random salt into a path. Frozen, for
/// the same reason the pairwise one is: a change would leave a shown
/// invitation's mailbox unreachable.
const DOMAIN: &[u8] = b"almena-id/invite/v1\0";

/// Where a code points when a camera app rather than a wallet reads it. The
/// portal serves that route, and the edge routes it; a wallet reading the
/// same code takes the `_oob` and ignores the rest of the URL.
const INVITATION_URL: &str = "https://almena.id/invite";

/// The DIDComm profile this wallet speaks.
const PROFILE: &str = "didcomm/v2";

/// The invitation the wallet is currently showing, as the book keeps it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    /// The random salt the key is walked to, base64.
    pub salt: String,
    /// The invitation's own id, which whoever answers it may thread on.
    pub id: String,
    /// The `did:peer:2` the code carries.
    pub did: String,
    /// The mediator the key's mailbox is at, and the one the invitation names.
    pub mediator: String,
}

impl Invite {
    /// A fresh invitation at `mediator`, from randomness the system supplies.
    pub fn draw(seed: &[u8; 64], mediator: &str) -> Result<Self, super::MessagingError> {
        let mut salt = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(salt.as_mut()).map_err(|_| super::MessagingError::Entropy)?;
        let key = derive(seed, &salt, mediator);
        Ok(Self {
            salt: B64.encode(salt.as_slice()),
            id: uuid::Uuid::new_v4().to_string(),
            did: key.did,
            mediator: mediator.to_owned(),
        })
    }

    /// The key again, from the salt and the seed.
    pub fn key(&self, seed: &[u8; 64]) -> Result<Pairwise, super::MessagingError> {
        let salt: [u8; 32] = B64
            .decode(&self.salt)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(super::MessagingError::Unreadable)?;
        Ok(derive(seed, &salt, &self.mediator))
    }

    /// The invitation as DIDComm v2 writes it: an out-of-band invitation
    /// from the key, saying only what protocol it speaks. No `goal` and no
    /// label — a wallet's invitation says nothing about whose it is.
    pub fn message(&self) -> Value {
        json!({
            "type": OOB_INVITATION,
            "id": self.id,
            "from": self.did,
            "body": { "accept": [PROFILE] },
        })
    }

    /// The invitation as a URL, which is what the code carries.
    pub fn url(&self) -> String {
        let encoded = URL_SAFE_NO_PAD.encode(self.message().to_string());
        format!("{INVITATION_URL}?_oob={encoded}")
    }
}

/// The invitation key `salt` walks to, named as a `did:peer:2` that carries
/// `mediator` as its service.
fn derive(seed: &[u8; 64], salt: &[u8; 32], mediator: &str) -> Pairwise {
    let secret = Zeroizing::new(keys::walk(
        seed,
        &pairwise::indices(keys::INVITE, DOMAIN, salt),
    ));
    Pairwise::from_secret(&secret, |signing_key, agreement_key| {
        let did = peer_did(signing_key, agreement_key, mediator);
        Named {
            signing_kid: format!("{did}#key-1"),
            agreement_kid: format!("{did}#key-2"),
            did,
        }
    })
}

/// `did:peer:2.V<signing>.E<agreement>.S<service>`: the two keys, and a
/// DIDCommMessaging service whose endpoint is the mediator's DID — which is
/// how DIDComm says "write to me through them". The service is JSON in the
/// method's abbreviations, base64url without padding.
fn peer_did(signing_key: &str, agreement_key: &str, mediator: &str) -> String {
    let service = json!({ "t": "dm", "s": { "uri": mediator, "a": [PROFILE] } });
    let encoded = URL_SAFE_NO_PAD.encode(service.to_string());
    format!("did:peer:2.V{signing_key}.E{agreement_key}.S{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use didcomm::did::ServiceKind;

    const SEED: [u8; 64] = [7u8; 64];
    const MEDIATOR: &str = "did:web:mediator.test";

    #[test]
    fn the_same_salt_walks_to_the_same_key() {
        let one = derive(&SEED, &[1u8; 32], MEDIATOR);
        let again = derive(&SEED, &[1u8; 32], MEDIATOR);
        let other = derive(&SEED, &[2u8; 32], MEDIATOR);
        assert_eq!(one.did, again.did);
        assert_ne!(one.did, other.did);
        assert!(one.did.starts_with("did:peer:2.Vz6Mk"));
    }

    #[test]
    fn the_key_resolves_to_what_it_holds() {
        // The library matches secrets to the document by id, so the two
        // have to agree on the names, and the document has to say where
        // the key is written to.
        let key = derive(&SEED, &[3u8; 32], MEDIATOR);
        let doc = super::super::did::peer(&key.did).unwrap();
        assert_eq!(doc.authentication, vec![format!("{}#key-1", key.did)]);
        assert_eq!(doc.key_agreement, vec![format!("{}#key-2", key.did)]);
        let ServiceKind::DIDCommMessaging { value } = &doc.service[0].service_endpoint else {
            panic!("not a messaging service");
        };
        assert_eq!(value.uri, MEDIATOR);
    }

    #[test]
    fn the_url_carries_the_invitation_and_the_salt_walks_back_to_the_key() {
        let invite = Invite::draw(&SEED, MEDIATOR).unwrap();
        let url = url::Url::parse(&invite.url()).unwrap();
        let (_, encoded) = url.query_pairs().find(|(n, _)| n == "_oob").unwrap();
        let json: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded.as_bytes()).unwrap()).unwrap();
        assert_eq!(json["type"], OOB_INVITATION);
        assert_eq!(json["from"], invite.did);
        assert_eq!(json["id"], invite.id);
        assert!(json["body"].get("goal").is_none());
        assert_eq!(invite.key(&SEED).unwrap().did, invite.did);
    }

    #[test]
    fn another_wallet_does_not_read_it_as_an_invitation() {
        // Holders do not talk to holders: what this wallet shows is for an
        // entity, and the reader that opens relationships refuses it.
        let invite = Invite::draw(&SEED, MEDIATOR).unwrap();
        assert!(super::super::invitation::read(&invite.url()).is_err());
    }

    #[test]
    fn an_invitation_is_not_the_identity() {
        let invite = Invite::draw(&SEED, MEDIATOR).unwrap();
        let identity = keys::signing_key(&SEED).verifying_key();
        assert!(!invite.did.contains(&keys::written(&identity.to_bytes())));
        assert!(!invite.url().contains(&keys::written(&identity.to_bytes())));
    }
}
