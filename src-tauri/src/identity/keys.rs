//! From the words to the key that governs the identity, and to how it is written.
//!
//! **The derivation path is frozen.** It decides which key a phrase produces, so
//! changing it after anybody has created an identity would make the same words
//! open a different, empty one — with no error anywhere to explain it. It
//! is SLIP-0010 over ed25519, one hardened step, pinned here against the same
//! constants the rest of the platform is pinned to.

use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use sha2::Sha512;
use zeroize::{Zeroize, Zeroizing};

/// SLIP-0010's key for the master step, fixed by that specification.
const MASTER_KEY: &[u8] = b"ed25519 seed";

/// The hardened index of the key that governs the identity: `m/0'`.
///
/// One step for the identity itself. The step is hardened because SLIP-0010 has
/// no unhardened derivation on this curve.
const CONTROL: u32 = 0;

/// The root of every pairwise key: `m/1'/…`. What comes after it is decided
/// by the counterparty — see `messaging::pairwise` — so that the same words
/// and the same counterparty always meet at the same key, on any device, with
/// nothing to remember in between.
pub(crate) const PAIRWISE: u32 = 1;

/// The key the messaging book is sealed under: `m/2'`. Not a signing key and
/// never used as one; a branch of its own so that nothing that signs shares
/// bytes with something that encrypts.
pub(crate) const BOOK: u32 = 2;

/// The root of every invitation key: `m/3'/…`. What comes after it is a hash
/// of a salt drawn at random when the invitation is shown — see
/// `messaging::invite` — because an invitation is for whoever scans it, and
/// there is no counterparty yet to derive from. The salt, not the key, is
/// what the book keeps.
pub(crate) const INVITE: u32 = 3;

/// What marks 32 bytes as an ed25519 public key: the multicodec `ed25519-pub`,
/// `0xed`, as an unsigned varint — the same two bytes `did:key` uses.
pub(crate) const ED25519_PUB: [u8; 2] = [0xed, 0x01];
/// And an x25519 one, `x25519-pub`: the key-agreement key a `did:key` derives
/// from its ed25519 key, which is what DIDComm encrypts towards.
pub(crate) const X25519_PUB: [u8; 2] = [0xec, 0x01];

/// The key the identity is, from the seed the words produce.
pub fn signing_key(seed: &[u8; 64]) -> SigningKey {
    // Wrapped on the way in, so the scalar the key is built from is wiped rather
    // than left on the stack. The key itself zeroes on drop; the copy it was made
    // from is what would otherwise linger.
    let secret = Zeroizing::new(walk(seed, &[CONTROL]));
    SigningKey::from_bytes(&secret)
}

/// A public key as text: multibase base58btc over the multicodec-prefixed bytes.
///
/// This is the form `did:key` is built from, and the form every key is shown in.
pub fn written(key: &[u8; 32]) -> String {
    multibase(&ED25519_PUB, key)
}

/// `z` + base58btc of a multicodec prefix followed by the raw key: the
/// `publicKeyMultibase` form of the Multikey data model, and the body of a
/// `did:key`.
pub(crate) fn multibase(codec: &[u8], key: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(codec.len() + key.len());
    bytes.extend_from_slice(codec);
    bytes.extend_from_slice(key);
    format!("z{}", bs58::encode(bytes).into_string())
}

/// Walk a path of hardened SLIP-0010 steps down from the seed.
///
/// The seed arrives as a slice and not as the 64 bytes BIP-39 makes, which is
/// the one concession this file makes to being checkable from outside itself:
/// SLIP-0010 publishes its vectors for seeds of other lengths, and a walk that
/// could not be handed one could only ever be tested against its own output.
/// Everything above this still passes exactly 64 bytes.
pub(crate) fn walk(seed: &[u8], path: &[u32]) -> [u8; 32] {
    // The master key and chain code. Both are worth as much as the seed:
    // whoever holds them derives every key these words will ever produce, so
    // each pair is wiped as the walk leaves it behind.
    let master = Zeroizing::new(hmac_sha512(MASTER_KEY, &[seed]));
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

    /// **The vectors SLIP-0010 publishes for ed25519, both of them, every node.**
    ///
    /// This is what says the walk above is the standard's and not merely its
    /// own. Everything else here asks whether the derivation is consistent with
    /// itself — the same verifier always answering the same key, two verifiers
    /// never sharing one — and a walk that split the HMAC output the wrong way
    /// round would pass all of it while producing keys no other implementation
    /// on earth arrives at. These twelve rows are the only thing that would
    /// catch that.
    ///
    /// It matters because the words are the whole of the backup. If the day
    /// comes that somebody has to recover an identity with something that is not
    /// this binary — a rescue tool, a second implementation, an audit — that
    /// works if and only if this walk is the published one.
    ///
    /// Each row is a seed, the hardened path below it, and what the standard
    /// says that path derives. The public key carries SLIP-0010's leading
    /// `00`, which is how that document writes an ed25519 public key and is not
    /// part of the key.
    const PUBLISHED: [(&str, &[u32], &str, &str); 12] = [
        // Test vector 1, whose seed is deliberately not 64 bytes long.
        (
            VECTOR_ONE,
            &[],
            "2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7",
            "00a4b2856bfec510abab89753fac1ac0e1112364e7d250545963f135f2a33188ed",
        ),
        (
            VECTOR_ONE,
            &[0],
            "68e0fe46dfb67e368c75379acec591dad19df3cde26e63b93a8e704f1dade7a3",
            "008c8a13df77a28f3445213a0f432fde644acaa215fc72dcdf300d5efaa85d350c",
        ),
        (
            VECTOR_ONE,
            &[0, 1],
            "b1d0bad404bf35da785a64ca1ac54b2617211d2777696fbffaf208f746ae84f2",
            "001932a5270f335bed617d5b935c80aedb1a35bd9fc1e31acafd5372c30f5c1187",
        ),
        (
            VECTOR_ONE,
            &[0, 1, 2],
            "92a5b23c0b8a99e37d07df3fb9966917f5d06e02ddbd909c7e184371463e9fc9",
            "00ae98736566d30ed0e9d2f4486a64bc95740d89c7db33f52121f8ea8f76ff0fc1",
        ),
        (
            VECTOR_ONE,
            &[0, 1, 2, 2],
            "30d1dc7e5fc04c31219ab25a27ae00b50f6fd66622f6e9c913253d6511d1e662",
            "008abae2d66361c879b900d204ad2cc4984fa2aa344dd7ddc46007329ac76c429c",
        ),
        (
            VECTOR_ONE,
            &[0, 1, 2, 2, 1_000_000_000],
            "8f94d394a8e8fd6b1bc2f3f49f5c47e385281d5c17e65324b0f62483e37e8793",
            "003c24da049451555d51a7014a37337aa4e12d41e485abccfa46b47dfb2af54b7a",
        ),
        // Test vector 2, whose seed is 64 bytes, as a phrase produces.
        (
            VECTOR_TWO,
            &[],
            "171cb88b1b3c1db25add599712e36245d75bc65a1a5c9e18d76f9f2b1eab4012",
            "008fe9693f8fa62a4305a140b9764c5ee01e455963744fe18204b4fb948249308a",
        ),
        (
            VECTOR_TWO,
            &[0],
            "1559eb2bbec5790b0c65d8693e4d0875b1747f4970ae8b650486ed7470845635",
            "0086fab68dcb57aa196c77c5f264f215a112c22a912c10d123b0d03c3c28ef1037",
        ),
        (
            VECTOR_TWO,
            &[0, 2_147_483_647],
            "ea4f5bfe8694d8bb74b7b59404632fd5968b774ed545e810de9c32a4fb4192f4",
            "005ba3b9ac6e90e83effcd25ac4e58a1365a9e35a3d3ae5eb07b9e4d90bcf7506d",
        ),
        (
            VECTOR_TWO,
            &[0, 2_147_483_647, 1],
            "3757c7577170179c7868353ada796c839135b3d30554bbb74a4b1e4a5a58505c",
            "002e66aa57069c86cc18249aecf5cb5a9cebbfd6fadeab056254763874a9352b45",
        ),
        (
            VECTOR_TWO,
            &[0, 2_147_483_647, 1, 2_147_483_646],
            "5837736c89570de861ebc173b1086da4f505d4adb387c6a1b1342d5e4ac9ec72",
            "00e33c0f7d81d843c572275f287498e8d408654fdf0d1e065b84e2e6f157aab09b",
        ),
        (
            VECTOR_TWO,
            &[0, 2_147_483_647, 1, 2_147_483_646, 2],
            "551d333177df541ad876a60ea71f00447931c0a9da16f227c11ea080d7391b8d",
            "0047150c75db263559a70d5778bf36abbab30fb061ad69f69ece61a72b0cfa4fc0",
        ),
    ];

    /// The seed SLIP-0010's first ed25519 vector is derived from.
    const VECTOR_ONE: &str = "000102030405060708090a0b0c0d0e0f";

    /// The seed of the second, which is 64 bytes — the length a phrase makes.
    const VECTOR_TWO: &str = "fffcf9f6f3f0edeae7e4e1dedbd8d5d2cfccc9c6c3c0bdbab7b4b1aeaba8a5a2\
9f9c999693908d8a8784817e7b7875726f6c696663605d5a5754514e4b484542";

    #[test]
    fn the_walk_is_the_one_the_standard_publishes() {
        for (seed, path, private, public) in PUBLISHED {
            let derived = walk(&decode(seed), path);
            assert_eq!(
                encode(&derived),
                private,
                "the private key at m{}",
                named(path)
            );

            // And that the key ed25519 builds from it is the one the standard
            // says, which is what ties this walk to the identifier it ends at.
            let verifying = SigningKey::from_bytes(&derived).verifying_key();
            assert_eq!(
                format!("00{}", encode(&verifying.to_bytes())),
                public,
                "the public key at m{}",
                named(path)
            );
        }
    }

    /// A path as SLIP-0010 writes it, for a failure that says which node broke.
    fn named(path: &[u32]) -> String {
        path.iter().map(|index| format!("/{index}H")).collect()
    }

    fn decode(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hexadecimal"))
            .collect()
    }

    fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
