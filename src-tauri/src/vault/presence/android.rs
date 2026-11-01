//! Android's keeper: the data key sealed under the Keystore's biometric key
//! (`almena-keystore`, `biometric_*`), the sealed copy in a private file beside
//! the record.
//!
//! The key is generated with `setUserAuthenticationRequired`, strong biometrics
//! only and for one operation at a time (`AUTH_BIOMETRIC_STRONG`, timeout 0;
//! per-use authentication before API 30), and
//! `setInvalidatedByBiometricEnrollment`. So the cipher that seals or opens the
//! copy does nothing until `BiometricPrompt` has recognised somebody, and a
//! fingerprint or face enrolled afterwards makes the copy one nothing opens —
//! which [`changed`] reads off the Keystore without a prompt, and the vault
//! turns into biometrics switched off.
//!
//! The copy is left out of backups and device transfers like the record
//! (`src-tauri/backup/`): the key that opens it never leaves the phone.

use std::fs;
use std::io::Write as _;

use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

use super::{Key, Prompt};
use crate::vault::{store, VaultError};

/// The sealed copy, next to `vault.json`.
const FILE: &str = "vault.biometric";

pub fn available<R: Runtime>(app: &AppHandle<R>) -> bool {
    almena_keystore::biometric_available(app)
}

/// Seals `key` once the person is recognised, and writes the copy down.
pub fn arm<R: Runtime>(app: &AppHandle<R>, key: &Key, prompt: &Prompt) -> Result<(), VaultError> {
    let sealed = almena_keystore::biometric_arm(app, key, &words(prompt)).map_err(failure)?;
    write(app, &sealed)
}

/// The data key, once the person is recognised.
pub fn open<R: Runtime>(app: &AppHandle<R>, prompt: &Prompt) -> Result<Zeroizing<Key>, VaultError> {
    let sealed = read(app)?.ok_or(VaultError::NoDeviceKey)?;
    let opened = Zeroizing::new(
        almena_keystore::biometric_open(app, &sealed, &words(prompt)).map_err(failure)?,
    );
    let key: Key = opened
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::DeviceChanged)?;
    Ok(Zeroizing::new(key))
}

/// Deletes the biometric key and the copy it sealed.
pub fn forget<R: Runtime>(app: &AppHandle<R>) -> Result<(), VaultError> {
    let _ = almena_keystore::biometric_forget(app);
    let path = store::beside(app, FILE)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(VaultError::NoDeviceStore),
    }
}

/// Whether there is a copy the Keystore can no longer open.
pub fn changed<R: Runtime>(app: &AppHandle<R>) -> bool {
    matches!(read(app), Ok(Some(sealed)) if almena_keystore::biometric_changed(app, &sealed))
}

fn words(prompt: &Prompt) -> almena_keystore::Words<'_> {
    almena_keystore::Words {
        title: &prompt.title,
        subtitle: &prompt.subtitle,
        cancel: &prompt.cancel,
    }
}

fn failure(error: almena_keystore::Error) -> VaultError {
    match error {
        almena_keystore::Error::Cancelled => VaultError::DeviceRefused,
        almena_keystore::Error::Changed => VaultError::DeviceChanged,
        almena_keystore::Error::Unavailable => VaultError::NoDeviceStore,
        almena_keystore::Error::Failed => VaultError::DeviceRefused,
    }
}

fn read<R: Runtime>(app: &AppHandle<R>) -> Result<Option<Vec<u8>>, VaultError> {
    match fs::read(store::beside(app, FILE)?) {
        Ok(sealed) => Ok(Some(sealed)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(VaultError::Storage),
    }
}

/// Written beside itself and moved into place, readable by this user alone, as
/// the record is.
fn write<R: Runtime>(app: &AppHandle<R>, sealed: &[u8]) -> Result<(), VaultError> {
    let path = store::beside(app, FILE)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| VaultError::Storage)?;
    }
    let temporary = path.with_extension("biometric.writing");
    let mut handle = store::create_private(&temporary).map_err(|_| VaultError::Storage)?;
    handle.write_all(sealed).map_err(|_| VaultError::Storage)?;
    handle.sync_all().map_err(|_| VaultError::Storage)?;
    drop(handle);
    fs::rename(&temporary, &path).map_err(|_| VaultError::Storage)
}
