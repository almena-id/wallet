//! The record: what the wallet writes down, and what it takes to read it back.
//!
//! **One secret, wrapped twice.** A random data key encrypts the seed; the data
//! key itself is encrypted by a key the PIN derives. Where the platform has a
//! secret store, a copy of the data key goes there too, so a face opens the
//! wallet without the digits — see [`super::store`]. Either wrap opens it and
//! neither is the wallet, which is what lets a failed sensor not be the end of
//! an identity and a forgotten PIN not be either.
//!
//! **The format carries its version and its own parameters.** A wallet installed
//! over an older one has to read what that one wrote, so nothing here is implied
//! by the code that reads it: the cost parameters, the salt and the nonces are
//! all in the record. A record from a newer wallet is refused by name rather
//! than misread.
//!
//! Nothing here reaches out. It turns bytes into a record and a record back into
//! bytes, which is what makes it testable without a device under it.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{VaultError, LENGTHS};

/// The version this wallet writes. A record numbered higher was written by a
/// wallet that knows something this one does not, and is refused rather than
/// guessed at.
pub const VERSION: u32 = 1;

/// The only key derivation this version has a name for.
const ARGON2ID: &str = "argon2id";

/// How much memory the derivation is made to touch, in kibibytes.
///
/// **This is the whole of the protection on a four digit PIN**, so it is written
/// into the record rather than assumed: raising it later must not lock out a
/// wallet that was sealed with less. 64 MiB with three passes lands around a
/// third of a second on a mid-range phone, which is a wait somebody accepts once
/// per launch and an attacker pays for every one of a million guesses.
const MEMORY_KIB: u32 = 64 * 1024;
const ITERATIONS: u32 = 3;
const PARALLELISM: u32 = 1;

/// The most this wallet will ever be told to spend.
///
/// **The cost is read from the record before anything about the record has been
/// authenticated**, and it has to be: the tag that would vouch for it can only
/// be checked with the key the cost is used to derive. Argon2 itself imposes no
/// upper bound — `Params::new` checks only that the memory is not too small —
/// and the derivation allocates that memory up front. So a record naming four
/// billion kibibytes would not fail a check, it would ask for four terabytes and
/// take the wallet down with it, on launch, before anybody could type a PIN.
///
/// These are bounds and not the parameters: the wallet writes far less, and the
/// room above what it writes is there so the cost can be raised later without
/// this having to move.
const MAX_MEMORY_KIB: u32 = 256 * 1024;
const MAX_ITERATIONS: u32 = 16;
const MAX_PARALLELISM: u32 = 4;

/// Lengths of the two secrets the record is built from.
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
const SALT_BYTES: usize = 16;
pub const SEED_BYTES: usize = 64;

/// How many wrong PINs a record survives.
///
/// Ten and not three: a pocket or a curious child must not be able to wipe an
/// identity, and what a low count would be defending against — a million guesses
/// made offline against a copied record — is already answered by the derivation
/// cost above and by where the record is kept.
pub const ATTEMPTS: u8 = 10;

/// A ciphertext and the nonce it was made with.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sealed {
    nonce: String,
    ciphertext: String,
}

/// The cost the PIN was derived at, as it was at the time of sealing.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kdf {
    algorithm: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

impl Kdf {
    /// Whether this is a cost the wallet may attempt at all.
    ///
    /// Checked before the derivation runs rather than after, because after is
    /// too late: the memory is claimed on the way in.
    fn affordable(&self) -> Result<(), VaultError> {
        let sane = (Params::MIN_M_COST..=MAX_MEMORY_KIB).contains(&self.memory_kib)
            && (Params::MIN_T_COST..=MAX_ITERATIONS).contains(&self.iterations)
            && (Params::MIN_P_COST..=MAX_PARALLELISM).contains(&self.parallelism);

        if sane {
            Ok(())
        } else {
            Err(VaultError::Unreadable)
        }
    }
}

/// Everything the wallet writes down about an identity.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// The format this record is in. Checked before anything else is believed.
    pub version: u32,
    kdf: Kdf,
    /// The seed, under the data key.
    seed: Sealed,
    /// The data key, under the key the PIN derives.
    pin_wrap: Sealed,
    /// How long the PIN is, so the keypad can be drawn before it is typed.
    pub digits: u8,
    /// Whether a copy of the data key was placed in the platform's secret store.
    pub device_wrap: bool,
    /// Wrong PINs since the last right one.
    pub wrong: u8,
}

impl Record {
    /// Seals a seed under a new data key and a PIN.
    ///
    /// # Errors
    ///
    /// [`VaultError::Entropy`] when the system will not supply randomness.
    pub fn seal(
        seed: &[u8; SEED_BYTES],
        pin: &str,
        digits: u8,
    ) -> Result<(Self, Zeroizing<[u8; KEY_BYTES]>), VaultError> {
        let data_key = Zeroizing::new(random::<KEY_BYTES>()?);
        let salt = random::<SALT_BYTES>()?;

        let kdf = Kdf {
            algorithm: ARGON2ID.to_string(),
            memory_kib: MEMORY_KIB,
            iterations: ITERATIONS,
            parallelism: PARALLELISM,
            salt: B64.encode(salt),
        };

        // The parameters are what the ciphertext is bound to, so a record whose
        // cost was rewritten down to nothing no longer opens: the wrap is
        // authenticated over the header that names them.
        let wrapping = derive(&kdf, pin)?;
        let pin_wrap = seal_with(&wrapping, &*data_key, &header(VERSION, &kdf))?;
        let sealed_seed = seal_with(&data_key, seed, SEED_CONTEXT)?;

        Ok((
            Self {
                version: VERSION,
                kdf,
                seed: sealed_seed,
                pin_wrap,
                digits,
                device_wrap: false,
                wrong: 0,
            },
            data_key,
        ))
    }

    /// The data key this PIN unwraps.
    ///
    /// # Errors
    ///
    /// [`VaultError::WrongPin`] when the digits do not open it, which is also
    /// what a record somebody has edited answers.
    pub fn unwrap_pin(&self, pin: &str) -> Result<Zeroizing<[u8; KEY_BYTES]>, VaultError> {
        let wrapping = derive(&self.kdf, pin)?;
        let opened = open_with(&wrapping, &self.pin_wrap, &header(self.version, &self.kdf))
            .ok_or(VaultError::WrongPin)?;

        key_from(&opened).ok_or(VaultError::Unreadable)
    }

    /// The seed this data key uncovers, however the key was come by.
    ///
    /// # Errors
    ///
    /// [`VaultError::Unreadable`] when the record does not open under it, which
    /// is what a device key left over from another identity looks like.
    pub fn seed(
        &self,
        data_key: &[u8; KEY_BYTES],
    ) -> Result<Zeroizing<[u8; SEED_BYTES]>, VaultError> {
        let opened = open_with(data_key, &self.seed, SEED_CONTEXT).ok_or(VaultError::Unreadable)?;
        let bytes: [u8; SEED_BYTES] = opened
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::Unreadable)?;

        Ok(Zeroizing::new(bytes))
    }

    /// The same seed and the same data key, wrapped by a different PIN.
    ///
    /// The seed is left exactly as it was: changing a PIN rewraps the key that
    /// opens the record and touches nothing that was sealed under it.
    ///
    /// # Errors
    ///
    /// [`VaultError::Entropy`] when the system will not supply a fresh salt.
    pub fn rewrap(
        &self,
        data_key: &[u8; KEY_BYTES],
        pin: &str,
        digits: u8,
    ) -> Result<Self, VaultError> {
        let salt = random::<SALT_BYTES>()?;
        let kdf = Kdf {
            algorithm: ARGON2ID.to_string(),
            memory_kib: MEMORY_KIB,
            iterations: ITERATIONS,
            parallelism: PARALLELISM,
            salt: B64.encode(salt),
        };

        let wrapping = derive(&kdf, pin)?;
        let pin_wrap = seal_with(&wrapping, data_key, &header(VERSION, &kdf))?;

        Ok(Self {
            version: VERSION,
            kdf,
            seed: self.seed.clone(),
            pin_wrap,
            digits,
            device_wrap: self.device_wrap,
            wrong: 0,
        })
    }

    /// Reads a record, refusing anything this version does not understand.
    ///
    /// # Errors
    ///
    /// [`VaultError::TooNew`] for a record a later wallet wrote, and
    /// [`VaultError::Unreadable`] for one that is not a record at all.
    pub fn parse(bytes: &[u8]) -> Result<Self, VaultError> {
        let record: Self = serde_json::from_slice(bytes).map_err(|_| VaultError::Unreadable)?;

        if record.version > VERSION {
            return Err(VaultError::TooNew);
        }
        // Zero is not a version this wallet ever wrote. It is what a record
        // somebody assembled by hand looks like, and reading one as if it were
        // version one would mean trusting a header nothing here produced.
        if record.version == 0 || record.kdf.algorithm != ARGON2ID {
            return Err(VaultError::Unreadable);
        }
        // The keypad is drawn from this before anything is typed, so a record
        // claiming a length this wallet does not offer would put a lock on
        // screen that nobody can answer.
        if !LENGTHS.contains(&record.digits) {
            return Err(VaultError::Unreadable);
        }
        record.kdf.affordable()?;

        Ok(record)
    }

    /// The bytes to write down.
    pub fn write(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a record is always serialisable")
    }

    /// How many wrong PINs are left before the record is destroyed.
    pub const fn left(&self) -> u8 {
        ATTEMPTS.saturating_sub(self.wrong)
    }
}

/// What the wrap is authenticated over: the version and the cost, so neither can
/// be rewritten without the wrap ceasing to open.
fn header(version: u32, kdf: &Kdf) -> Vec<u8> {
    format!(
        "almena-vault/{version}/{}/{}/{}/{}/{}",
        kdf.algorithm, kdf.memory_kib, kdf.iterations, kdf.parallelism, kdf.salt
    )
    .into_bytes()
}

/// What the seed itself is authenticated over. It is sealed under a key that is
/// never reused, so it needs no more than to say which of the two wraps it is.
const SEED_CONTEXT: &[u8] = b"almena-vault/seed";

fn derive(kdf: &Kdf, pin: &str) -> Result<Zeroizing<[u8; KEY_BYTES]>, VaultError> {
    // Twice, because `parse` is not the only way in: a record built in this
    // process goes straight here, and the day somebody adds a third caller the
    // bound should already be under it.
    kdf.affordable()?;

    let params = Params::new(
        kdf.memory_kib,
        kdf.iterations,
        kdf.parallelism,
        Some(KEY_BYTES),
    )
    .map_err(|_| VaultError::Unreadable)?;
    let salt = B64.decode(&kdf.salt).map_err(|_| VaultError::Unreadable)?;

    let mut derived = Zeroizing::new([0u8; KEY_BYTES]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), &salt, &mut *derived)
        .map_err(|_| VaultError::Unreadable)?;

    Ok(derived)
}

fn seal_with(key: &[u8; KEY_BYTES], plaintext: &[u8], aad: &[u8]) -> Result<Sealed, VaultError> {
    let nonce = random::<NONCE_BYTES>()?;
    let ciphertext = XChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Unreadable)?;

    Ok(Sealed {
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })
}

fn open_with(key: &[u8; KEY_BYTES], sealed: &Sealed, aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    let nonce = B64.decode(&sealed.nonce).ok()?;
    let ciphertext = B64.decode(&sealed.ciphertext).ok()?;

    XChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(
            XNonce::from_slice(nonce.as_slice().get(..NONCE_BYTES)?),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .ok()
        .map(Zeroizing::new)
}

fn key_from(bytes: &[u8]) -> Option<Zeroizing<[u8; KEY_BYTES]>> {
    let key: [u8; KEY_BYTES] = bytes.try_into().ok()?;
    Some(Zeroizing::new(key))
}

fn random<const N: usize>() -> Result<[u8; N], VaultError> {
    let mut bytes = [0u8; N];
    getrandom::getrandom(&mut bytes).map_err(|_| VaultError::Entropy)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; SEED_BYTES] = [42u8; SEED_BYTES];

    fn sealed() -> (Record, Zeroizing<[u8; KEY_BYTES]>) {
        Record::seal(&SEED, "1234", 4).expect("entropy")
    }

    #[test]
    fn the_pin_that_sealed_it_opens_it_and_no_other_does() {
        let (record, _) = sealed();

        let key = record.unwrap_pin("1234").expect("the right PIN");
        assert_eq!(*record.seed(&key).expect("the seed"), SEED);

        assert!(matches!(
            record.unwrap_pin("1235"),
            Err(VaultError::WrongPin)
        ));
    }

    #[test]
    fn a_record_survives_being_written_and_read_back() {
        let (record, _) = sealed();
        // The whole point of the format: a wallet installed over this one has to
        // read what this one wrote.
        let read = Record::parse(&record.write()).expect("a record this version wrote");

        let key = read.unwrap_pin("1234").expect("the right PIN");
        assert_eq!(*read.seed(&key).expect("the seed"), SEED);
        assert_eq!(read.digits, 4);
    }

    #[test]
    fn a_record_from_a_later_wallet_is_refused_rather_than_misread() {
        let (record, _) = sealed();
        let mut written: serde_json::Value = serde_json::from_slice(&record.write()).expect("json");
        written["version"] = serde_json::json!(VERSION + 1);

        assert!(matches!(
            Record::parse(&serde_json::to_vec(&written).expect("json")),
            Err(VaultError::TooNew)
        ));
    }

    #[test]
    fn the_cost_cannot_be_rewritten_down_to_nothing() {
        // Everything a cheap attack would want to change is authenticated: an
        // attacker who lowers the memory cost gets a record that no longer opens
        // rather than one that opens quickly.
        let (record, _) = sealed();
        let mut written: serde_json::Value = serde_json::from_slice(&record.write()).expect("json");
        written["kdf"]["memoryKib"] = serde_json::json!(8);

        let tampered =
            Record::parse(&serde_json::to_vec(&written).expect("json")).expect("still a record");
        assert!(matches!(
            tampered.unwrap_pin("1234"),
            Err(VaultError::WrongPin)
        ));
    }

    #[test]
    fn changing_the_pin_leaves_the_identity_untouched() {
        let (record, key) = sealed();
        let changed = record.rewrap(&key, "654321", 6).expect("entropy");

        let reopened = changed.unwrap_pin("654321").expect("the new PIN");
        assert_eq!(*changed.seed(&reopened).expect("the seed"), SEED);
        assert_eq!(changed.digits, 6);
        // And the old one stops working, which is the whole of what changing it means.
        assert!(matches!(
            changed.unwrap_pin("1234"),
            Err(VaultError::WrongPin)
        ));
    }

    #[test]
    fn a_data_key_from_another_identity_uncovers_nothing() {
        let (record, _) = sealed();
        let (other, other_key) = Record::seal(&[9u8; SEED_BYTES], "1234", 4).expect("entropy");

        assert!(matches!(
            record.seed(&other_key),
            Err(VaultError::Unreadable)
        ));
        drop(other);
    }

    #[test]
    fn a_record_that_names_a_length_the_keypad_has_no_answer_for_is_refused() {
        let (record, _) = sealed();
        let mut written: serde_json::Value = serde_json::from_slice(&record.write()).expect("json");
        written["digits"] = serde_json::json!(9);

        assert!(matches!(
            Record::parse(&serde_json::to_vec(&written).expect("json")),
            Err(VaultError::Unreadable)
        ));
    }

    #[test]
    fn a_cost_no_device_could_pay_is_refused_before_it_is_attempted() {
        // The one check that cannot wait for the authentication tag: the tag is
        // verified with a key this cost is used to derive, so a record naming
        // four billion kibibytes would take the wallet down on launch rather
        // than be rejected.
        let (record, _) = sealed();
        for (field, absurd) in [
            ("memoryKib", u32::MAX),
            ("memoryKib", 0),
            ("iterations", u32::MAX),
            ("iterations", 0),
            ("parallelism", u32::MAX),
        ] {
            let mut written: serde_json::Value =
                serde_json::from_slice(&record.write()).expect("json");
            written["kdf"][field] = serde_json::json!(absurd);

            assert!(
                matches!(
                    Record::parse(&serde_json::to_vec(&written).expect("json")),
                    Err(VaultError::Unreadable)
                ),
                "{field} = {absurd}"
            );
        }
    }

    #[test]
    fn the_cost_this_wallet_writes_is_one_it_will_pay() {
        let (record, _) = sealed();
        Record::parse(&record.write()).expect("a record this version wrote");
    }

    #[test]
    fn a_record_numbered_zero_was_written_by_nobody() {
        let (record, _) = sealed();
        let mut written: serde_json::Value = serde_json::from_slice(&record.write()).expect("json");
        written["version"] = serde_json::json!(0);

        assert!(matches!(
            Record::parse(&serde_json::to_vec(&written).expect("json")),
            Err(VaultError::Unreadable)
        ));
    }

    #[test]
    fn a_nonce_of_the_wrong_length_is_refused_rather_than_panicking() {
        // Every byte of the record is somebody else's to edit once it is a file.
        let (record, _) = sealed();
        for replacement in ["", "AAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] {
            let mut written: serde_json::Value =
                serde_json::from_slice(&record.write()).expect("json");
            written["pinWrap"]["nonce"] = serde_json::json!(replacement);

            let tampered = Record::parse(&serde_json::to_vec(&written).expect("json"))
                .expect("still a record");
            assert!(
                matches!(tampered.unwrap_pin("1234"), Err(VaultError::WrongPin)),
                "nonce {replacement:?}"
            );
        }
    }

    #[test]
    fn attempts_count_down_from_ten() {
        let (mut record, _) = sealed();
        assert_eq!(record.left(), ATTEMPTS);
        record.wrong = 3;
        assert_eq!(record.left(), ATTEMPTS - 3);
        record.wrong = 200;
        assert_eq!(record.left(), 0);
    }
}
