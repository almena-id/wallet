//! Windows Hello, for the vault.
//!
//! **Windows keeps no secret behind Hello; it keeps a key that signs.**
//! `KeyCredentialManager` makes an RSA key pair inside the TPM (or Hello's
//! software container where there is none), bound to this user, and the private
//! half is used only once Hello has recognised somebody — face, finger or the
//! Hello PIN — and never leaves. There is no item to put thirty-two bytes in,
//! as there is in a Keychain, so the data key is wrapped under a key *derived
//! from a signature* instead:
//!
//! 1. On arming, a random 32-byte challenge is drawn, Hello signs it, and
//!    HKDF-SHA256 (the challenge as salt) turns the signature into the key that
//!    wraps the data key with XChaCha20-Poly1305.
//! 2. The challenge and the wrapped key — nothing else — go into the Credential
//!    Manager under their own entry, beside the record.
//! 3. On unlocking, Hello signs the same challenge, which gives the same
//!    signature, the same wrapping key and the data key back.
//!
//! **Why that is sound.** Hello signs with RSASSA-PKCS1-v1_5 over SHA-256,
//! which is deterministic: one key and one message give one signature, every
//! time (RFC 8017 §8.2). So the signature is a fixed secret function of the
//! challenge that only the TPM can compute, and only after Hello's own check.
//! What is stored — the challenge and a ciphertext — is what any process of the
//! user can read out of the Credential Manager, and neither opens anything
//! without that signature; the private key that would make it never leaves the
//! TPM, so a copy of the entry is nothing on another machine either. Should a
//! signature ever come back different (a key that is not PKCS#1 v1.5, a TPM
//! that misbehaves), the wrap's tag fails and it reads as changed: biometrics
//! turn off and the PIN, which never depended on any of this, opens the wallet.
//!
//! **Two prompts to arm, one to unlock.** Creating the Hello key asks for Hello,
//! and so does the signature that proves it. Neither call takes a window to
//! parent its prompt to: `KeyCredentialManager` has no `…ForWindow` variant and
//! no interop interface (only `UserConsentVerifier` has one,
//! `IUserConsentVerifierInterop`, and it yields a yes or no, not a key). The
//! prompt is the system's Credential UI, which normally comes to the front; if
//! it opens behind the wallet, it is in the taskbar.
//!
//! The derivation and the wrap are behind [`Signer`], so they are tested on
//! every host; only the WinRT signer is Windows'.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::Key;
use crate::vault::VaultError;

/// What a sealed copy starts with, and what its wrap is bound to.
const MAGIC: &[u8] = b"almena-hello/1\n";
/// What HKDF is told the key is for, so it is good for nothing else.
const INFO: &[u8] = b"almena-wallet/hello/1";
const CHALLENGE_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
/// The data key and the Poly1305 tag.
const WRAPPED_BYTES: usize = 32 + 16;

/// Something that signs a challenge once the person has been recognised.
pub trait Signer {
    /// The signature of `challenge`.
    ///
    /// # Errors
    ///
    /// [`VaultError::DeviceRefused`] when nobody was recognised,
    /// [`VaultError::DeviceChanged`] when the key is gone.
    fn sign(&self, challenge: &[u8]) -> Result<Vec<u8>, VaultError>;
}

/// `key`, wrapped under what `signer`'s signature of `challenge` derives: the
/// copy to keep, `MAGIC ‖ challenge ‖ nonce ‖ wrapped`.
pub fn seal(
    signer: &impl Signer,
    key: &Key,
    challenge: &[u8; CHALLENGE_BYTES],
) -> Result<Vec<u8>, VaultError> {
    let wrapping = wrapping(&signer.sign(challenge)?, challenge);
    let nonce = crate::vault::record::random::<NONCE_BYTES>()?;
    let wrapped = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(wrapping.as_slice()))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: key,
                aad: MAGIC,
            },
        )
        .map_err(|_| VaultError::Unreadable)?;

    Ok([MAGIC, challenge, &nonce, &wrapped].concat())
}

/// The key [`seal`] wrapped, if `signer` still signs as it did.
///
/// # Errors
///
/// [`VaultError::DeviceChanged`] for a copy that is not one, or that this
/// signature does not open, and whatever the signer says.
pub fn unseal(signer: &impl Signer, sealed: &[u8]) -> Result<Zeroizing<Key>, VaultError> {
    let body = sealed
        .strip_prefix(MAGIC)
        .filter(|body| body.len() == CHALLENGE_BYTES + NONCE_BYTES + WRAPPED_BYTES)
        .ok_or(VaultError::DeviceChanged)?;
    let (challenge, rest) = body.split_at(CHALLENGE_BYTES);
    let (nonce, wrapped) = rest.split_at(NONCE_BYTES);

    let wrapping = wrapping(&signer.sign(challenge)?, challenge);
    let key = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(wrapping.as_slice()))
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: wrapped,
                aad: MAGIC,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| VaultError::DeviceChanged)?;

    let key: Key = key
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::DeviceChanged)?;
    Ok(Zeroizing::new(key))
}

/// HKDF-SHA256 of the signature, salted with the challenge it signs.
fn wrapping(signature: &[u8], challenge: &[u8]) -> Zeroizing<Key> {
    let mut key = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(challenge), signature)
        .expand(INFO, key.as_mut_slice())
        .expect("32 bytes is a length HKDF-SHA256 can expand to");
    key
}

#[cfg(windows)]
pub use system::{arm, available, changed, forget, open};

/// Windows' half: `KeyCredentialManager` and the Credential Manager entry.
///
/// Every WinRT call is blocked on with `get()`. These run from the vault's
/// commands, which are `async` and so off the interface thread, where blocking
/// on an asynchronous operation is allowed.
#[cfg(windows)]
mod system {
    use windows::core::HSTRING;
    use windows::Security::Credentials::{
        KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
    };
    use windows::Security::Cryptography::CryptographicBuffer;
    use zeroize::Zeroizing;

    use super::{seal, unseal, Signer, CHALLENGE_BYTES};
    use crate::vault::presence::Key;
    use crate::vault::store::SERVICE;
    use crate::vault::VaultError;

    /// The Hello key's name, the same for every launch of this wallet.
    const NAME: &str = "almena-wallet";
    /// The Credential Manager entry holding the challenge and the wrapped key,
    /// apart from the record's.
    const ENTRY: &str = "hello-key";

    struct Hello(KeyCredential);

    impl Signer for Hello {
        fn sign(&self, challenge: &[u8]) -> Result<Vec<u8>, VaultError> {
            let data = CryptographicBuffer::CreateFromByteArray(challenge)
                .map_err(|_| VaultError::Storage)?;
            let result = self
                .0
                .RequestSignAsync(&data)
                .and_then(|operation| operation.get())
                .map_err(|_| VaultError::DeviceRefused)?;
            match result.Status() {
                Ok(KeyCredentialStatus::Success) => {}
                Ok(KeyCredentialStatus::NotFound) => return Err(VaultError::DeviceChanged),
                _ => return Err(VaultError::DeviceRefused),
            }
            let signature = result.Result().map_err(|_| VaultError::DeviceRefused)?;
            let mut bytes = windows::core::Array::<u8>::new();
            CryptographicBuffer::CopyToByteArray(&signature, &mut bytes)
                .map_err(|_| VaultError::DeviceRefused)?;
            Ok(bytes.to_vec())
        }
    }

    /// Whether Hello is set up for this user: a PIN, a face or a finger.
    pub fn available() -> bool {
        KeyCredentialManager::IsSupportedAsync()
            .and_then(|operation| operation.get())
            .unwrap_or(false)
    }

    /// Makes a new Hello key — over any old one — and keeps the data key wrapped
    /// under its signature.
    pub fn arm(key: &Key) -> Result<(), VaultError> {
        let created = KeyCredentialManager::RequestCreateAsync(
            &HSTRING::from(NAME),
            KeyCredentialCreationOption::ReplaceExisting,
        )
        .and_then(|operation| operation.get())
        .map_err(|_| VaultError::NoDeviceStore)?;
        match created.Status() {
            Ok(KeyCredentialStatus::Success) => {}
            Ok(KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword) => {
                return Err(VaultError::DeviceUnproven)
            }
            _ => return Err(VaultError::NoDeviceStore),
        }
        let credential = created
            .Credential()
            .map_err(|_| VaultError::NoDeviceStore)?;

        let challenge = crate::vault::record::random::<CHALLENGE_BYTES>()?;
        let sealed = seal(&Hello(credential), key, &challenge)?;
        entry()?
            .set_secret(&sealed)
            .map_err(|_| VaultError::NoDeviceStore)
    }

    /// The data key, once Hello has signed the challenge again.
    pub fn open() -> Result<Zeroizing<Key>, VaultError> {
        let sealed = match entry()?.get_secret() {
            Ok(sealed) => sealed,
            Err(keyring_core::Error::NoEntry) => return Err(VaultError::NoDeviceKey),
            Err(_) => return Err(VaultError::Storage),
        };
        unseal(&Hello(credential()?), &sealed)
    }

    /// Deletes the Hello key and the entry. Neither being there already is fine.
    pub fn forget() -> Result<(), VaultError> {
        let _ = KeyCredentialManager::DeleteAsync(&HSTRING::from(NAME))
            .and_then(|operation| operation.get());
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err(VaultError::NoDeviceStore),
        }
    }

    /// Whether there is a wrapped key whose Hello key is gone — Hello was reset
    /// or removed. `OpenAsync` only looks the key up; it shows nothing.
    pub fn changed() -> bool {
        entry().is_ok_and(|entry| entry.get_secret().is_ok())
            && matches!(credential(), Err(VaultError::DeviceChanged))
    }

    fn credential() -> Result<KeyCredential, VaultError> {
        let opened = KeyCredentialManager::OpenAsync(&HSTRING::from(NAME))
            .and_then(|operation| operation.get())
            .map_err(|_| VaultError::DeviceRefused)?;
        match opened.Status() {
            Ok(KeyCredentialStatus::Success) => {
                opened.Credential().map_err(|_| VaultError::DeviceRefused)
            }
            Ok(KeyCredentialStatus::NotFound) => Err(VaultError::DeviceChanged),
            _ => Err(VaultError::DeviceRefused),
        }
    }

    fn entry() -> Result<keyring_core::Entry, VaultError> {
        keyring_core::Entry::new(SERVICE, ENTRY).map_err(|_| VaultError::NoDeviceStore)
    }
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};

    use super::*;

    /// Deterministic, as PKCS#1 v1.5 is: one secret, one message, one signature.
    struct Fixed(&'static [u8]);

    impl Signer for Fixed {
        fn sign(&self, challenge: &[u8]) -> Result<Vec<u8>, VaultError> {
            let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.0).unwrap();
            mac.update(challenge);
            Ok(mac.finalize().into_bytes().to_vec())
        }
    }

    /// A signature that changes every time: what a randomised scheme would do.
    struct Drifting(std::cell::Cell<u8>);

    impl Signer for Drifting {
        fn sign(&self, challenge: &[u8]) -> Result<Vec<u8>, VaultError> {
            self.0.set(self.0.get() + 1);
            Ok([challenge, &[self.0.get()]].concat())
        }
    }

    struct Refusing;

    impl Signer for Refusing {
        fn sign(&self, _: &[u8]) -> Result<Vec<u8>, VaultError> {
            Err(VaultError::DeviceRefused)
        }
    }

    const KEY: Key = [9u8; 32];
    const CHALLENGE: [u8; CHALLENGE_BYTES] = [3u8; CHALLENGE_BYTES];

    #[test]
    fn the_same_signature_of_the_same_challenge_gives_the_key_back() {
        let signer = Fixed(b"the TPM's key");
        let sealed = seal(&signer, &KEY, &CHALLENGE).unwrap();
        assert_eq!(*unseal(&signer, &sealed).unwrap(), KEY);
    }

    #[test]
    fn what_is_kept_holds_neither_the_key_nor_the_signature() {
        let signer = Fixed(b"the TPM's key");
        let sealed = seal(&signer, &KEY, &CHALLENGE).unwrap();
        let signature = signer.sign(&CHALLENGE).unwrap();
        assert!(sealed.starts_with(MAGIC));
        assert!(!sealed.windows(KEY.len()).any(|window| window == KEY));
        assert!(!sealed
            .windows(signature.len())
            .any(|window| window == signature.as_slice()));
        assert_eq!(&sealed[MAGIC.len()..][..CHALLENGE_BYTES], &CHALLENGE);
    }

    #[test]
    fn another_key_a_drifting_signature_or_a_tampered_copy_reads_as_changed() {
        let sealed = seal(&Fixed(b"the TPM's key"), &KEY, &CHALLENGE).unwrap();
        assert!(matches!(
            unseal(&Fixed(b"a new Hello key"), &sealed),
            Err(VaultError::DeviceChanged)
        ));

        let drifting = Drifting(std::cell::Cell::new(0));
        let drifted = seal(&drifting, &KEY, &CHALLENGE).unwrap();
        assert!(matches!(
            unseal(&drifting, &drifted),
            Err(VaultError::DeviceChanged)
        ));

        let signer = Fixed(b"the TPM's key");
        for at in [MAGIC.len(), MAGIC.len() + CHALLENGE_BYTES, sealed.len() - 1] {
            let mut tampered = sealed.clone();
            tampered[at] ^= 1;
            assert!(
                matches!(unseal(&signer, &tampered), Err(VaultError::DeviceChanged)),
                "byte {at}"
            );
        }
        assert!(matches!(
            unseal(&signer, &sealed[..sealed.len() - 1]),
            Err(VaultError::DeviceChanged)
        ));
        assert!(matches!(
            unseal(&signer, b"{}"),
            Err(VaultError::DeviceChanged)
        ));
    }

    #[test]
    fn nobody_recognised_seals_and_opens_nothing() {
        assert!(matches!(
            seal(&Refusing, &KEY, &CHALLENGE),
            Err(VaultError::DeviceRefused)
        ));
        let sealed = seal(&Fixed(b"the TPM's key"), &KEY, &CHALLENGE).unwrap();
        assert!(matches!(
            unseal(&Refusing, &sealed),
            Err(VaultError::DeviceRefused)
        ));
    }

    #[test]
    fn the_wrapping_key_follows_the_signature_and_the_challenge() {
        let one = wrapping(b"signature", &CHALLENGE);
        assert_eq!(*one, *wrapping(b"signature", &CHALLENGE));
        assert_ne!(*one, *wrapping(b"signaturf", &CHALLENGE));
        assert_ne!(*one, *wrapping(b"signature", &[4u8; CHALLENGE_BYTES]));
    }
}
