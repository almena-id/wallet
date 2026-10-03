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

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

#[cfg(target_os = "android")]
pub use android::{open, seal, Error};

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

    /// The Keystore would not do it, or there is no key to open with — after a
    /// restore from a backup, for instance, which never carries the key.
    #[derive(Debug)]
    pub struct Error;

    #[derive(Serialize, Deserialize)]
    struct Bytes {
        data: String,
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
        let keystore = app.try_state::<Keystore<R>>().ok_or(Error)?;
        let answer: Bytes = keystore
            .0
            .run_mobile_plugin(
                command,
                Bytes {
                    data: B64.encode(bytes),
                },
            )
            .map_err(|_| Error)?;
        B64.decode(answer.data).map_err(|_| Error)
    }
}
