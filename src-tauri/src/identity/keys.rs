//! From the words to the key that governs the identity, and to how it is written.
//!
//! **The derivation path is frozen.** It decides which key a phrase produces, so
//! changing it after anybody has created an identity would make the same twelve
//! words open a different, empty one — with no error anywhere to explain it. It
//! is SLIP-0010 over ed25519, one hardened step, pinned here against the same
//! constants the rest of the platform is pinned to.

use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256, Sha512};
use zeroize::{Zeroize, Zeroizing};

/// SLIP-0010's key for the master step, fixed by that specification.
const MASTER_KEY: &[u8] = b"ed25519 seed";

/// The hardened index of the key that governs the identity: `m/0'`.
///
/// One step, not a wallet-style tree. This wallet derives one key, its meaning
/// is fixed, and depth beyond that would only be somewhere for two
/// implementations to disagree. The step is hardened because SLIP-0010 has no
/// unhardened derivation on this curve.
const CONTROL: u32 = 0;

/// What marks 32 bytes as an ed25519 public key: the multicodec `ed25519-pub`,
/// `0xed`, as an unsigned varint — the same two bytes `did:key` uses.
const ED25519_PUB: [u8; 2] = [0xed, 0x01];

/// How many steps a verifier's own key hangs below the control key.
///
/// Eight, because that is what it takes to spend the digest of a verifier's
/// identifier: SLIP-0010 indices carry 31 bits each, and eight of them consume
/// 248 of the 256. Fewer steps would leave the rest of the digest unused, and
/// two verifiers whose keys collided would be handed the same identifier —
/// which is the one thing deriving per verifier exists to prevent.
const SITE_STEPS: usize = 8;

/// The key the identity is, from the seed the words produce.
pub fn signing_key(seed: &[u8; 64]) -> SigningKey {
    // Wrapped on the way in, so the scalar the key is built from is wiped rather
    // than left on the stack. The key itself zeroes on drop; the copy it was made
    // from is what would otherwise linger.
    let secret = Zeroizing::new(walk(seed, &[CONTROL]));
    SigningKey::from_bytes(&secret)
}

/// The key this identity uses at one verifier, and nowhere else.
///
/// **This is what keeps two sites from recognising the same person.** Every
/// verifier is shown a different public key, and the only thing that knows they
/// belong together is this device. Nothing is stored to make that work: the
/// path is the digest of the verifier's identifier, so the same phrase on a new
/// phone derives exactly the same keys again, having saved nothing.
///
/// The identifier the path is taken from is the verifier's DID, which the wallet
/// reads from a signed request and can resolve for itself — not a label anybody
/// asserted in passing.
pub fn site_key(seed: &[u8; 64], verifier: &str) -> SigningKey {
    let mut path = Vec::with_capacity(1 + SITE_STEPS);
    path.push(CONTROL);
    path.extend_from_slice(&site_path(verifier));

    let secret = Zeroizing::new(walk(seed, &path));
    SigningKey::from_bytes(&secret)
}

/// The steps below the control key that one verifier's identifier names.
fn site_path(verifier: &str) -> [u32; SITE_STEPS] {
    let digest = Sha256::digest(verifier.as_bytes());
    let mut path = [0u32; SITE_STEPS];

    for (step, chunk) in digest.chunks_exact(4).take(SITE_STEPS).enumerate() {
        let bytes: [u8; 4] = chunk.try_into().expect("four bytes at a time");
        // The top bit is what hardens a step, so an index only has 31 to give.
        path[step] = u32::from_be_bytes(bytes) & 0x7fff_ffff;
    }

    path
}

/// A public key as text: multibase base58btc over the multicodec-prefixed bytes.
///
/// This is the form `did:key` is built from, and the form every key is shown in.
pub fn written(key: &[u8; 32]) -> String {
    let mut bytes = Vec::with_capacity(ED25519_PUB.len() + key.len());
    bytes.extend_from_slice(&ED25519_PUB);
    bytes.extend_from_slice(key);
    format!("z{}", bs58::encode(bytes).into_string())
}

/// Walk a path of hardened SLIP-0010 steps down from the seed.
fn walk(seed: &[u8; 64], path: &[u32]) -> [u8; 32] {
    // The master key and chain code. Both are worth as much as the seed:
    // whoever holds them derives every key these words will ever produce, so
    // each pair is wiped as the walk leaves it behind.
    let master = Zeroizing::new(hmac_sha512(MASTER_KEY, &[seed.as_slice()]));
    let (mut key, mut chain) = split(*master);

    for index in path {
        // SLIP-0010 hardened child: 0x00 || key || (index + 2^31), big endian.
        let hardened = (index | 0x8000_0000).to_be_bytes();
        let stepped = Zeroizing::new(hmac_sha512(
            chain.as_slice(),
            &[&[0u8], key.as_slice(), &hardened],
        ));

        key.zeroize();
        chain.zeroize();
        (key, chain) = split(*stepped);
    }

    chain.zeroize();
    key
}

fn hmac_sha512(key: &[u8], parts: &[&[u8]]) -> [u8; 64] {
    // The only failure of `new_from_slice` for HMAC is a key length it refuses,
    // and HMAC accepts every length.
    let mut mac = <Hmac<Sha512>>::new_from_slice(key).expect("HMAC accepts any key length");
    for part in parts {
        mac.update(part);
    }
    mac.finalize().into_bytes().into()
}

/// SLIP-0010 splits every 64 byte result into a key and a chain code.
fn split(bytes: [u8; 64]) -> ([u8; 32], [u8; 32]) {
    let mut key = [0u8; 32];
    let mut chain = [0u8; 32];
    key.copy_from_slice(&bytes[..32]);
    chain.copy_from_slice(&bytes[32..]);
    (key, chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seed of no significance: what matters is that the same one keeps
    /// producing the same keys.
    const SEED: [u8; 64] = [7u8; 64];

    const ONE: &str = "did:web:almena.id:v0000000000000001";
    const ANOTHER: &str = "did:web:almena.id:v0000000000000002";

    #[test]
    fn a_verifier_always_gets_the_same_key() {
        // A phrase restored on a new device has to arrive at the same
        // identifiers, having stored nothing to remember them by.
        assert_eq!(
            site_key(&SEED, ONE).verifying_key().to_bytes(),
            site_key(&SEED, ONE).verifying_key().to_bytes()
        );
    }

    #[test]
    fn two_verifiers_are_shown_different_keys() {
        // The whole point: neither site can tell it is the same person, and
        // neither can the platform.
        assert_ne!(
            site_key(&SEED, ONE).verifying_key().to_bytes(),
            site_key(&SEED, ANOTHER).verifying_key().to_bytes()
        );
    }

    #[test]
    fn a_verifiers_key_is_not_the_identity_itself() {
        assert_ne!(
            site_key(&SEED, ONE).verifying_key().to_bytes(),
            signing_key(&SEED).verifying_key().to_bytes()
        );
    }

    #[test]
    fn the_path_spends_the_whole_digest() {
        // Anything less and two verifiers could land on one key, which would
        // hand them both the same person.
        let path = site_path(ONE);
        assert_eq!(path.len(), SITE_STEPS);
        assert!(path.iter().all(|index| *index < 0x8000_0000));
        assert_ne!(path, site_path(ANOTHER));
    }
}
