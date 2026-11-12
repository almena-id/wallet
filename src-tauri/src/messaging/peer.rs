//! The DIDs this wallet speaks as, and the keys behind them.
//!
//! **Every one is derived from the seed, none is drawn at random**, so the same
//! words always make the same DIDs and nothing secret has to be written down
//! beside them. Each is a `did:peer:2` with an Ed25519 authentication key, an
//! X25519 key agreement key, and one service that routes through the mediator —
//! which is what lets a sender wrap a message in a `forward` for it. The
//! service is part of the DID, so the same keys through another mediator are
//! another DID.
//!
//! **Every path starts with the mediator.** `x/y/z` below come from a hash of
//! the mediator's DID, so the keys — not only the DID's service — differ from
//! one mediator to another: no two mediators, nor a relationship moved from
//! one to another, ever show the same public key, and none of them can tell
//! it is the same wallet.
//!
//! - **The inbox** (`m/1'/x/y/z`): the DID the mediation is keyed by. It
//!   talks to the mediator and nobody else ever sees it.
//! - **The contact card** (`m/4'/x/y/z`): the DID the wallet's invitation
//!   through that mediator names, the one somebody writes to first. Shared on
//!   purpose; see `contacts`.
//! - **A pairwise** (`m/3'/x/y/z/a/b/c`): one per counterparty, `a/b/c` from a
//!   hash of the counterparty's DID. It is the only DID that counterparty ever
//!   learns, so no two of them can tell they are talking to the same person.

use almena_didcomm::did::peer::{peer2, Purpose};
use almena_didcomm::{Curve, InMemorySecrets, SecretKey};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::MessagingError;
use crate::identity::keys::{self, CARD, INBOX, PAIRWISE};

/// Key ids `peer2` gives the keys, in the order they are passed to it.
const AUTHENTICATION: &str = "#key-1";
const KEY_AGREEMENT: &str = "#key-2";

/// Three hardened steps of 31 bits each from the SHA-256 of `text`: 93 bits,
/// collision-free for any number of mediators or counterparties.
fn steps(text: &str) -> [u32; 3] {
    let hash = Sha256::digest(text.as_bytes());
    let step = |at: usize| {
        u32::from_be_bytes([hash[at], hash[at + 1], hash[at + 2], hash[at + 3]]) & 0x7fff_ffff
    };
    [step(0), step(4), step(8)]
}

/// The path under `role` for `mediator`: the role, then the mediator.
fn base(role: u32, mediator: &str) -> Vec<u32> {
    [&[role][..], &steps(mediator)].concat()
}

/// A DID this wallet speaks as, with its secrets.
pub struct Peer {
    pub did: String,
    keys: [(String, SecretKey); 2],
}

impl Peer {
    /// The inbox: the DID the mediation with `mediator` is keyed by.
    pub fn inbox(seed: &[u8; 64], mediator: &str) -> Result<Self, MessagingError> {
        Self::derive(seed, &base(INBOX, mediator), mediator)
    }

    /// The contact card: the DID the invitation through `mediator` names.
    pub fn card(seed: &[u8; 64], mediator: &str) -> Result<Self, MessagingError> {
        Self::derive(seed, &base(CARD, mediator), mediator)
    }

    /// The pairwise DID for `counterparty` — the DID it was first known by —
    /// through `mediator`.
    pub fn pairwise(
        seed: &[u8; 64],
        mediator: &str,
        counterparty: &str,
    ) -> Result<Self, MessagingError> {
        let path = [base(PAIRWISE, mediator), steps(counterparty).to_vec()].concat();
        Self::derive(seed, &path, mediator)
    }

    /// The DID whose keys are at `base/0'` (Ed25519) and `base/1'` (X25519)
    /// and whose service routes through `mediator`.
    fn derive(seed: &[u8; 64], base: &[u32], mediator: &str) -> Result<Self, MessagingError> {
        let path = |leaf: u32| [base, &[leaf]].concat();
        let signing = SecretKey::from_bytes(Curve::Ed25519, &*keys::derive(seed, &path(0)))
            .map_err(|_| MessagingError::Keys)?;
        let agreement = SecretKey::from_bytes(Curve::X25519, &*keys::derive(seed, &path(1)))
            .map_err(|_| MessagingError::Keys)?;

        let service = json!({
            "type": "DIDCommMessaging",
            "serviceEndpoint": {"uri": mediator, "accept": ["didcomm/v2"]},
        });
        let did = peer2(
            &[
                (Purpose::Verification, &signing.public_key()),
                (Purpose::Encryption, &agreement.public_key()),
            ],
            &[service],
        )
        .map_err(|_| MessagingError::Keys)?;

        Ok(Self {
            keys: [
                (format!("{did}{AUTHENTICATION}"), signing),
                (format!("{did}{KEY_AGREEMENT}"), agreement),
            ],
            did,
        })
    }

    /// Its secrets, for packing as it or proving it.
    pub fn secrets(&self) -> InMemorySecrets {
        let mut secrets = InMemorySecrets::new();
        self.add_to(&mut secrets);
        secrets
    }

    /// Adds its secrets to a set that opens messages for several DIDs.
    pub fn add_to(&self, secrets: &mut InMemorySecrets) {
        for (kid, key) in &self.keys {
            secrets.insert(kid.clone(), key.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEDIATOR: &str = "did:web:mediator.example.com";
    const OTHER: &str = "did:web:other.example.com";

    /// The public keys a `did:peer:2` carries, without its service.
    fn keys_of(did: &str) -> Vec<&str> {
        did.trim_start_matches("did:peer:2")
            .split('.')
            .filter(|part| part.starts_with('V') || part.starts_with('E'))
            .collect()
    }

    #[test]
    fn the_same_seed_makes_the_same_dids() {
        let seed = [7u8; 64];
        for (first, second) in [
            (Peer::inbox(&seed, MEDIATOR), Peer::inbox(&seed, MEDIATOR)),
            (Peer::card(&seed, MEDIATOR), Peer::card(&seed, MEDIATOR)),
            (
                Peer::pairwise(&seed, MEDIATOR, "did:peer:2.x"),
                Peer::pairwise(&seed, MEDIATOR, "did:peer:2.x"),
            ),
        ] {
            let (first, second) = (first.expect("a peer"), second.expect("a peer"));
            assert_eq!(first.did, second.did);
            assert!(first.did.starts_with("did:peer:2"));
        }
    }

    #[test]
    fn every_role_and_every_counterparty_gets_its_own_did() {
        let seed = [7u8; 64];
        let dids = [
            Peer::inbox(&seed, MEDIATOR).expect("inbox").did,
            Peer::card(&seed, MEDIATOR).expect("card").did,
            Peer::pairwise(&seed, MEDIATOR, "did:peer:2.a")
                .expect("pairwise")
                .did,
            Peer::pairwise(&seed, MEDIATOR, "did:peer:2.b")
                .expect("pairwise")
                .did,
            Peer::inbox(&[8u8; 64], MEDIATOR).expect("inbox").did,
            Peer::inbox(&seed, OTHER).expect("inbox").did,
        ];
        for (at, did) in dids.iter().enumerate() {
            assert!(!dids[at + 1..].contains(did), "{did}");
        }
    }

    #[test]
    fn two_mediators_see_no_key_in_common() {
        let seed = [7u8; 64];
        let roles = |mediator: &str| {
            [
                Peer::inbox(&seed, mediator).expect("inbox").did,
                Peer::card(&seed, mediator).expect("card").did,
                Peer::pairwise(&seed, mediator, "did:peer:2.a")
                    .expect("pairwise")
                    .did,
            ]
        };
        for (here, there) in roles(MEDIATOR).iter().zip(roles(OTHER).iter()) {
            let (here, there) = (keys_of(here), keys_of(there));
            assert_eq!(here.len(), 2);
            assert!(here.iter().all(|key| !there.contains(key)));
        }
    }

    #[test]
    fn the_paths_are_as_documented() {
        let seed = [7u8; 64];
        let key = |did: String| keys_of(&did)[0].to_owned();
        let derived = |path: &[u32]| {
            let signing =
                SecretKey::from_bytes(Curve::Ed25519, &*keys::derive(&seed, path)).expect("a key");
            format!(
                "V{}",
                almena_didcomm::did::multikey::encode(&signing.public_key())
            )
        };
        let [x, y, z] = steps(MEDIATOR);
        let [a, b, c] = steps("did:peer:2.a");
        assert_eq!(
            key(Peer::inbox(&seed, MEDIATOR).expect("inbox").did),
            derived(&[INBOX, x, y, z, 0])
        );
        assert_eq!(
            key(Peer::card(&seed, MEDIATOR).expect("card").did),
            derived(&[CARD, x, y, z, 0])
        );
        assert_eq!(
            key(Peer::pairwise(&seed, MEDIATOR, "did:peer:2.a")
                .expect("pairwise")
                .did),
            derived(&[PAIRWISE, x, y, z, a, b, c, 0])
        );
        // The steps: SHA-256, three big-endian words, 31 bits each.
        assert_eq!(steps(MEDIATOR), [1_342_901_657, 877_905_330, 1_272_677_530]);
    }

    #[tokio::test]
    async fn a_peer_resolves_to_its_keys_and_its_mediator() {
        use almena_didcomm::{DidResolver, LocalResolver};

        let peer = Peer::card(&[7u8; 64], MEDIATOR).expect("a peer");
        let document = LocalResolver::new()
            .resolve(&peer.did)
            .await
            .expect("a document");

        assert!(document
            .authentication_key(&format!("{}{AUTHENTICATION}", peer.did))
            .is_ok());
        assert!(document
            .key_agreement_key(&format!("{}{KEY_AGREEMENT}", peer.did))
            .is_ok());
        let endpoints = document
            .didcomm_services()
            .next()
            .expect("a service")
            .didcomm_endpoints()
            .expect("endpoints");
        assert_eq!(endpoints[0].uri, MEDIATOR);
    }
}
