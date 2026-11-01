//! The Android Keystore, for the vault.
//!
//! On Android there is no secret store the Rust side can reach, so the vault's
//! record is a file in the application's private storage. That file is already
//! encrypted under the PIN, but four or six digits are few: whoever copied it
//! off the phone could try them all with no limit. This seals the file a second
//! time, under an AES-256-GCM key generated inside the Android Keystore —
//! StrongBox where the phone has one, its TEE otherwise — which never leaves
//! that hardware. A copy of the file is then nothing without the phone, and the
//! count of wrong PINs inside it cannot be edited.
//!
//! Two calls, [`seal`] and [`open`], made by the vault from Rust; nothing in the
//! webview can reach them. On every other platform they do not exist.
//!
//! **Biometrics are a second key.** Also AES-256-GCM in the Keystore, StrongBox
//! where there is one, but usable only for one operation at a time that a
//! strong biometric (Class 3: a fingerprint, or a face the phone rates as
//! strong) has just unlocked through `BiometricPrompt`, and invalidated by the
//! system the moment another fingerprint or face is enrolled. The vault seals
//! its data key under it when biometrics are turned on ([`biometric_arm`]) and
//! opens it to unlock ([`biometric_open`]); the sealed copy is the vault's to
//! keep. A key the system invalidated answers [`Error::Changed`], which the
//! vault treats as biometrics turned off: the PIN still opens the record.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

#[cfg(target_os = "android")]
pub use android::{
    biometric_arm, biometric_available, biometric_changed, biometric_forget, biometric_open, open,
    seal, Error, Words,
};

/// Registers the plugin. On Android it loads the Kotlin side; elsewhere it is
/// an empty plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("almena-keystore")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle =
                    _api.register_android_plugin("id.almena.keystore", "KeystorePlugin")?;
                _app.manage(android::Keystore(handle));
            }
            Ok(())
        })
        .build()
}

#[cfg(target_os = "android")]
mod android {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use serde::{Deserialize, Serialize};
    use tauri::plugin::PluginHandle;
    use tauri::{AppHandle, Manager as _, Runtime};

    pub struct Keystore<R: Runtime>(pub PluginHandle<R>);

    /// Why the Keystore did not do it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Error {
        /// It would not, or there is no key to open with — after a restore from
        /// a backup, for instance, which never carries the key.
        Failed,
        /// The person closed the prompt, or chose the PIN instead.
        Cancelled,
        /// The biometric key is gone or invalidated: a fingerprint or face was
        /// enrolled since, or the screen lock was removed.
        Changed,
        /// This phone has no strong biometric enrolled.
        Unavailable,
    }

    /// The words `BiometricPrompt` shows, in the interface's language.
    pub struct Words<'a> {
        pub title: &'a str,
        pub subtitle: &'a str,
        /// The negative button: "Use PIN".
        pub cancel: &'a str,
    }

    #[derive(Serialize, Deserialize)]
    struct Bytes {
        data: String,
    }

    #[derive(Serialize)]
    struct Prompted<'a> {
        data: String,
        title: &'a str,
        subtitle: &'a str,
        cancel: &'a str,
    }

    #[derive(Deserialize)]
    struct Answer {
        answer: bool,
    }

    /// Whether this phone has a strong biometric enrolled
    /// (`BiometricManager.canAuthenticate(BIOMETRIC_STRONG)`).
    pub fn biometric_available<R: Runtime>(app: &AppHandle<R>) -> bool {
        ask(app, "biometricAvailable", None)
    }

    /// `key` sealed under a new biometric key, once `BiometricPrompt` has
    /// recognised somebody: the 12-byte nonce, then the ciphertext and its tag.
    /// Any older biometric key is deleted first.
    pub fn biometric_arm<R: Runtime>(
        app: &AppHandle<R>,
        key: &[u8],
        words: &Words<'_>,
    ) -> Result<Vec<u8>, Error> {
        prompted(app, "biometricArm", key, words)
    }

    /// What [`biometric_arm`] sealed, once `BiometricPrompt` has recognised
    /// somebody.
    pub fn biometric_open<R: Runtime>(
        app: &AppHandle<R>,
        sealed: &[u8],
        words: &Words<'_>,
    ) -> Result<Vec<u8>, Error> {
        prompted(app, "biometricOpen", sealed, words)
    }

    /// Whether `sealed` can no longer be opened — the key gone or invalidated.
    /// Asks nothing of the person: the Keystore says so before any prompt.
    pub fn biometric_changed<R: Runtime>(app: &AppHandle<R>, sealed: &[u8]) -> bool {
        ask(app, "biometricChanged", Some(sealed))
    }

    /// Deletes the biometric key.
    pub fn biometric_forget<R: Runtime>(app: &AppHandle<R>) -> Result<(), Error> {
        let keystore = app.try_state::<Keystore<R>>().ok_or(Error::Failed)?;
        keystore
            .0
            .run_mobile_plugin::<serde_json::Value>("biometricForget", ())
            .map(|_| ())
            .map_err(reason)
    }

    fn ask<R: Runtime>(app: &AppHandle<R>, command: &str, bytes: Option<&[u8]>) -> bool {
        let Some(keystore) = app.try_state::<Keystore<R>>() else {
            return false;
        };
        let data = bytes.map(|bytes| B64.encode(bytes)).unwrap_or_default();
        keystore
            .0
            .run_mobile_plugin::<Answer>(command, Bytes { data })
            .is_ok_and(|answer| answer.answer)
    }

    fn prompted<R: Runtime>(
        app: &AppHandle<R>,
        command: &str,
        bytes: &[u8],
        words: &Words<'_>,
    ) -> Result<Vec<u8>, Error> {
        let keystore = app.try_state::<Keystore<R>>().ok_or(Error::Failed)?;
        let answer: Bytes = keystore
            .0
            .run_mobile_plugin(
                command,
                Prompted {
                    data: B64.encode(bytes),
                    title: words.title,
                    subtitle: words.subtitle,
                    cancel: words.cancel,
                },
            )
            .map_err(reason)?;
        B64.decode(answer.data).map_err(|_| Error::Failed)
    }

    /// The code the Kotlin side rejected with (`KeystorePlugin.kt`).
    fn reason(failure: tauri::plugin::mobile::PluginInvokeError) -> Error {
        match failure {
            tauri::plugin::mobile::PluginInvokeError::InvokeRejected(response) => {
                match response.code.as_deref() {
                    Some("cancelled") => Error::Cancelled,
                    Some("changed") => Error::Changed,
                    Some("unavailable") => Error::Unavailable,
                    _ => Error::Failed,
                }
            }
            _ => Error::Failed,
        }
    }

    /// `plain` sealed under the device's key, made the first time it is asked
    /// for.
    pub fn seal<R: Runtime>(app: &AppHandle<R>, plain: &[u8]) -> Result<Vec<u8>, Error> {
        call(app, "seal", plain)
    }

    /// What [`seal`] sealed, if this device's key opens it.
    pub fn open<R: Runtime>(app: &AppHandle<R>, sealed: &[u8]) -> Result<Vec<u8>, Error> {
        call(app, "open", sealed)
    }

    fn call<R: Runtime>(app: &AppHandle<R>, command: &str, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        let keystore = app.try_state::<Keystore<R>>().ok_or(Error::Failed)?;
        let answer: Bytes = keystore
            .0
            .run_mobile_plugin(
                command,
                Bytes {
                    data: B64.encode(bytes),
                },
            )
            .map_err(|_| Error::Failed)?;
        B64.decode(answer.data).map_err(|_| Error::Failed)
    }
}
