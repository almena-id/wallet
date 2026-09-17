//! One key per relationship, decided by who the relationship is with.
//!
//! **The counterparty is the path.** A pairwise identifier is derived from the
//! seed and from the DID it is a relationship with — `m/1'/a'/b'/c'/d'`, the
//! four indices being a hash of that DID — so that the same words meet the
//! same counterparty at the same key on any device, with no counter to keep
//! and nothing to restore. That is what makes the spec's promise true that
//! somebody who types their phrase into a new wallet is recognised by the
//! issuers they already dealt with: the wallet does not need to remember which
//! index was whose, only who it has relationships with.
//!
//! Hardened steps only, as SLIP-0010 has on this curve. Nothing that sees one
//! pairwise key, public or private, can reach another or the root: they share
//! an ancestor, and an ancestor is not something a child gives away.
//!
//! What a pairwise carries is what DIDComm needs to reach it: the Ed25519 key
//! the `did:key` is, and the X25519 key derived from it the way the method
//! specifies, which is what somebody encrypts towards.

use didcomm::secrets::{Secret, SecretMaterial, SecretType};
use ed25519_dalek::SigningKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::identity::keys::{self, X25519_PUB};

/// Salt under the hash that turns a counterparty into a path. Frozen: a change
/// would move every relationship to a key nobody else knows.
const DOMAIN: &[u8] = b"almena-id/pairwise/v1\0";

/// A relationship's key, held for as long as one exchange needs it.
pub struct Pairwise {
    /// `did:key:z6Mk…`, the identifier this wallet is in this relationship.
    pub did: String,
    /// The private keys, in the shape the DIDComm library asks for them.
    secrets: Vec<Secret>,
}

impl Pairwise {
    /// The pairwise the seed produces for `counterparty`.
    pub fn derive(seed: &[u8; 64], counterparty: &str) -> Self {
        let secret = Zeroizing::new(keys::walk(seed, &path(counterparty)));
        let signing = SigningKey::from_bytes(&secret);
        let verifying = signing.verifying_key();

        let signing_key = keys::written(&verifying.to_bytes());
        let did = format!("did:key:{signing_key}");

        // The X25519 pair the method derives: the public key is the Edwards
        // point mapped to Montgomery form, the private key is the clamped
        // scalar the Ed25519 secret hashes to. Both are what `did:key` says
        // they are, so what is published by derivation is what can be opened.
        let agreement_secret = Zeroizing::new(signing.to_scalar_bytes());
        let agreement_public = verifying.to_montgomery();
        let agreement_key = keys::multibase(&X25519_PUB, agreement_public.as_bytes());

        let secrets = vec![
            secret_of(
                &format!("{did}#{signing_key}"),
                jwk("Ed25519", verifying.as_bytes(), &signing.to_bytes()),
            ),
            secret_of(
                &format!("{did}#{agreement_key}"),
                jwk(
                    "X25519",
                    agreement_public.as_bytes(),
                    agreement_secret.as_slice(),
                ),
            ),
        ];

        Self { did, secrets }
    }

    /// The private keys, for the library.
    pub fn secrets(&self) -> Secrets {
        Secrets(self.secrets.clone())
    }
}

/// `m/1'/a'/b'/c'/d'`: the pairwise root, then four indices carved out of the
/// hash of the counterparty. Thirty-one bits each, because a hardened index is
/// the index plus 2^31 and the top bit is the hardening.
fn path(counterparty: &str) -> [u32; 5] {
    let digest = Sha256::new()
        .chain_update(DOMAIN)
        .chain_update(counterparty.as_bytes())
        .finalize();
    let index = |at: usize| {
        u32::from_be_bytes([digest[at], digest[at + 1], digest[at + 2], digest[at + 3]])
            & 0x7FFF_FFFF
    };
    [keys::PAIRWISE, index(0), index(4), index(8), index(12)]
}

/// An in-memory `SecretsResolver`: the library asks it for the private key
/// behind a `kid` when unpacking, and when authenticating what it packs.
#[derive(Clone)]
pub struct Secrets(Vec<Secret>);

#[async_trait::async_trait]
impl didcomm::secrets::SecretsResolver for Secrets {
    async fn get_secret(&self, secret_id: &str) -> didcomm::error::Result<Option<Secret>> {
        Ok(self.0.iter().find(|s| s.id == secret_id).cloned())
    }

    async fn find_secrets<'a>(
        &self,
        secret_ids: &'a [&'a str],
    ) -> didcomm::error::Result<Vec<&'a str>> {
        Ok(secret_ids
            .iter()
            .copied()
            .filter(|id| self.0.iter().any(|s| s.id == *id))
            .collect())
    }
}

fn secret_of(id: &str, private_key_jwk: Value) -> Secret {
    Secret {
        id: id.to_owned(),
        type_: SecretType::JsonWebKey2020,
        secret_material: SecretMaterial::JWK { private_key_jwk },
    }
}

fn jwk(curve: &str, public: &[u8], private: &[u8]) -> Value {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    json!({
        "kty": "OKP",
        "crv": curve,
        "x": URL_SAFE_NO_PAD.encode(public),
        "d": URL_SAFE_NO_PAD.encode(private),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 64] = [7u8; 64];

    #[test]
    fn the_same_counterparty_always_meets_the_same_key() {
        let one = Pairwise::derive(&SEED, "did:web:almena.id:city-hall");
        let again = Pairwise::derive(&SEED, "did:web:almena.id:city-hall");
        assert_eq!(one.did, again.did);
        assert!(one.did.starts_with("did:key:z6Mk"));
    }

    #[test]
    fn two_counterparties_never_share_a_key() {
        let one = Pairwise::derive(&SEED, "did:web:almena.id:city-hall");
        let other = Pairwise::derive(&SEED, "did:web:almena.id:university");
        assert_ne!(one.did, other.did);
    }

    #[test]
    fn a_pairwise_is_not_the_identity() {
        let identity = keys::signing_key(&SEED).verifying_key();
        let pairwise = Pairwise::derive(&SEED, "did:web:almena.id");
        assert_ne!(
            pairwise.did,
            format!("did:key:{}", keys::written(&identity.to_bytes()))
        );
    }

    #[test]
    fn the_agreement_key_is_the_one_the_method_derives() {
        // The library resolves a did:key by derivation; the secret it is
        // handed for that key has to be the one the derivation names.
        let pairwise = Pairwise::derive(&SEED, "did:web:almena.id");
        let doc = super::super::did::key(&pairwise.did).unwrap();
        for secret in &pairwise.secrets {
            assert!(
                doc.verification_method.iter().any(|m| m.id == secret.id),
                "{} is not in the document",
                secret.id
            );
        }
        assert_eq!(doc.key_agreement.len(), 1);
    }
}
