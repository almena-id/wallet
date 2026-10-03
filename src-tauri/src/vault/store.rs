//! Where the record is kept, which is not the same place on every system.
//!
//! **The platform's own secret store first.** On iOS and macOS that is the
//! Keychain, on Windows the Credential Manager, on Linux the Secret Service. It
//! is not chosen for secrecy alone — the record is already encrypted — but for
//! what it survives: a Keychain item outlives the app being deleted, so a wallet
//! reinstalled on the same phone finds its identity where it left it, which a
//! file inside the application's own container would not.
//!
//! **The store is named, never guessed.** `keyring-core` is given exactly one
//! store per platform and answers `NoDefaultStore` where it was given none. The
//! umbrella `keyring` crate would instead pick one for you, and the one it picks
//! on Android is an in-memory stub: it would look like it was working and lose
//! the identity on every launch. Nothing here can fail that way.
//!
//! **Android has no store this side can reach**, so nothing is registered there
//! and the record goes to the application's private storage, which no other
//! application can read and which survives an update. Reaching the Android
//! Keystore needs a Kotlin bridge and a JNI symbol the wallet does not have yet;
//! until it does, this is said out loud rather than implied.
//!
//! **macOS has two keychains, and this module uses both.** They are not
//! alternatives; each one is here for the thing the other cannot do.
//!
//! - The **file keychain** holds the record. Every build can reach it — signed,
//!   ad-hoc signed, or not signed at all — so an identity is never held hostage
//!   by how the copy of the wallet in front of somebody happened to be built.
//! - The **data protection keychain** holds the device key, because it is the
//!   only one on macOS whose items can carry `kSecAccessControlUserPresence`.
//!   In the file keychain there is no such thing: it hands its items to whoever
//!   is logged in, with no prompt, which is why opening without the PIN was not
//!   offered on a computer before this.
//!
//!   That store answers only to a build signed with an App ID a provisioning
//!   profile authorises; anything else gets `errSecMissingEntitlement` back. So
//!   it is asked once at startup, by putting an item in and taking it straight
//!   out again — see [`PROBE`] — and a build it will not answer for reports no
//!   device store rather than offering a switch that would fail on being used.
//!
//! Two things this is bound to on Apple platforms, and neither is obvious:
//!
//! - **A Keychain item belongs to the team and the bundle identifier together.**
//!   Changing `APPLE_DEVELOPMENT_TEAM` or the identifier in `tauri.conf.json`
//!   gives the next build a different access group, and the record already on
//!   the phone becomes unreachable — not corrupted, not deleted, simply somebody
//!   else's. The way back is the phrase.
//! - **The record is pinned to the device it was written on**, and the device key
//!   to the person. `when-unlocked-this-device-only` keeps the record out of
//!   every backup and off every other phone. `require-user-presence` makes the
//!   operating system itself ask for a face before it hands over the key that
//!   opens the wallet without a PIN — which is why the prompt is not decoration
//!   on iOS: it is `kSecAccessControlUserPresence`, enforced by the system
//!   rather than by a screen this wallet drew.
//!
//!   The store cannot ask for the two together. `apple-native-keyring-store`
//!   maps `require-user-presence` onto plain `kSecAttrAccessibleWhenUnlocked`, so
//!   a device key armed beside a PIN is in the backup class and could be
//!   restored onto another phone. It opens nothing there: the record it would
//!   open is the one that stayed behind.
//!
//!   **Where the iPhone's lock is the only one, the key is written here
//!   instead** — [`set_device_lock`] — with both: the presence check and
//!   `when-passcode-set-this-device-only`. Face ID first, the passcode after
//!   it, never in a backup, and gone the moment the passcode is taken off the
//!   phone, at which point the phrase is the way back.
//!
//! Reading looks in both places, because a wallet whose store stopped answering
//! — a Linux session with no Secret Service running — must not conclude that
//! there is no identity. Writing goes to one place and clears the other, so
//! there is never a second, older copy to find.

#[cfg(any(target_os = "ios", target_os = "macos"))]
use std::collections::HashMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use tauri::{Manager, Runtime};

use zeroize::Zeroizing;

use super::VaultError;

/// Where a record was found, or where one was put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    /// The platform's secret store.
    Store,
    /// A file in the application's private directory.
    File,
    /// That file, sealed again under a key in the Android Keystore.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    Keystore,
}

impl Home {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::File => "file",
            Self::Keystore => "keystore",
        }
    }
}

/// The file the record falls back to, inside the application's own directory.
const FILE: &str = "vault.json";

/// What a file sealed under the Android Keystore starts with. A record itself
/// is JSON and starts with `{`, so the two cannot be mistaken for each other.
#[cfg(target_os = "android")]
const SEALED: &[u8] = b"almena-keystore/1\n";

/// The service every item is filed under. The bundle identifier, so the items
/// are recognisably this wallet's wherever somebody looks at them.
const SERVICE: &str = "id.almena.wallet";
const RECORD: &str = "vault";
#[cfg(any(target_os = "ios", target_os = "macos"))]
const DEVICE_KEY: &str = "device-key";

/// The account the macOS probe writes to and takes straight back out.
///
/// **It has to be a write.** Asking for an item that is not there answers "no
/// such item" on any build, entitled or not: the store looks for the item
/// before it looks at who is asking, so a read tells you nothing. Adding one is
/// where `errSecMissingEntitlement` comes back, so adding one is the question.
///
/// It carries no access policy, so there is no presence check to evaluate and
/// the probe cannot put a prompt on the screen at startup. It holds a byte that
/// means nothing, under an account nothing else uses, and it is deleted in the
/// same breath.
#[cfg(target_os = "macos")]
const PROBE: &str = "entitlement-probe";

/// What the device key is written behind: the system will not hand the item
/// back until it has recognised somebody.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn presence() -> HashMap<&'static str, &'static str> {
    HashMap::from([("access-policy", "require-user-presence")])
}

/// The macOS data protection keychain, once it has been asked whether it will
/// answer this build at all. `None` where it will not.
#[cfg(target_os = "macos")]
static PROTECTED: std::sync::OnceLock<Option<std::sync::Arc<keyring_core::CredentialStore>>> =
    std::sync::OnceLock::new();

/// Registers the one store this platform has, if it has one.
///
/// Called once at startup. A platform with nothing to register is left with
/// nothing, and every call below answers accordingly rather than pretending.
///
/// **macOS registers one and keeps a second.** The default store — the one
/// every `Entry::new` below reaches — is the file keychain, where the record
/// lives. The data protection keychain is not a default anything: it is held
/// aside for the single item that needs what only it has, and it is probed
/// before being kept, because a build without the entitlement would otherwise
/// look like a Mac that can open the wallet with a fingerprint right up to the
/// moment somebody tried it.
pub fn init() {
    #[cfg(target_os = "ios")]
    if let Ok(store) = apple_native_keyring_store::protected::Store::new() {
        keyring_core::set_default_store(store);
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(store) = apple_native_keyring_store::keychain::Store::new() {
            keyring_core::set_default_store(store);
        }

        let protected = apple_native_keyring_store::protected::Store::new()
            .ok()
            .map(|store| store as std::sync::Arc<keyring_core::CredentialStore>)
            .filter(|store| {
                // Whether this build is one the store will take an item from at
                // all — see `PROBE`. Put in and taken out again; what is left
                // behind is the answer and not the byte.
                store.build(SERVICE, PROBE, None).is_ok_and(|entry| {
                    let entitled = entry.set_secret(b"\0").is_ok();
                    let _ = entry.delete_credential();
                    entitled
                })
            });

        let _ = PROTECTED.set(protected);
    }

    #[cfg(windows)]
    if let Ok(store) = windows_native_keyring_store::store::Store::new() {
        keyring_core::set_default_store(store);
    }

    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    if let Ok(store) = zbus_secret_service_keyring_store::store::Store::new() {
        keyring_core::set_default_store(store);
    }
}

/// Whether there is somewhere to keep a device key **behind the system's own
/// presence check**.
///
/// Every platform here has somewhere to put thirty-two bytes; that is not what
/// is being asked. What is being asked is whether putting them there means the
/// operating system will stand in front of them, and the answer is no on a
/// store that hands its items to whoever is logged in.
pub fn has_device_store() -> bool {
    #[cfg(target_os = "ios")]
    {
        // The default store is the protected one, so having a store and having
        // somewhere to put this are the same question.
        keyring_core::get_default_store().is_some()
    }

    #[cfg(target_os = "macos")]
    {
        PROTECTED.get().is_some_and(Option::is_some)
    }

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    {
        false
    }
}

/// Reads the record, from wherever it is.
///
/// # Errors
///
/// [`VaultError::Storage`] when a place that should have answered did not — as
/// distinct from answering that it holds nothing.
pub fn read<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<(Vec<u8>, Home)>, VaultError> {
    // **"It holds nothing" and "it would not say" are different answers.** A
    // locked Linux keyring, or a macOS keychain prompt somebody dismissed, must
    // not come back as an empty device: the wallet would offer to create a
    // second identity over the one already there, and the first wrong PIN
    // afterwards would take the copy it had just written.
    match stored_record() {
        Ok(Some(bytes)) => return Ok(Some((bytes, Home::Store))),
        Ok(None) => {}
        Err(failure) => return Err(failure),
    }

    let path = file(app)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(VaultError::Storage),
    };

    #[cfg(target_os = "android")]
    {
        if let Some(sealed) = bytes.strip_prefix(SEALED) {
            // A key the Keystore no longer has — the file came back from a
            // backup, which never carries it — opens nothing: the phrase is
            // the way back, as for a lost phone.
            let record = almena_keystore::open(app, sealed).map_err(|_| VaultError::Unreadable)?;
            return Ok(Some((record, Home::Keystore)));
        }
        // Written before the Keystore was used: sealed now, so the copy that
        // could be guessed at offline does not stay on the phone.
        let home = write(app, &bytes).unwrap_or(Home::File);
        return Ok(Some((bytes, home)));
    }

    #[cfg(not(target_os = "android"))]
    Ok(Some((bytes, Home::File)))
}

/// Writes the record, and clears whatever copy the other place was holding.
///
/// # Errors
///
/// [`VaultError::Storage`] when neither place would take it.
pub fn write<R: Runtime>(app: &tauri::AppHandle<R>, bytes: &[u8]) -> Result<Home, VaultError> {
    if record_entry().is_ok_and(|entry| entry.set_secret(bytes).is_ok()) {
        // Not an error if it was not there: what matters is that it is not there
        // now.
        let _ = fs::remove_file(file(app)?);
        return Ok(Home::Store);
    }

    // **iOS does not fall back.** A file in the container would be the one copy
    // of the identity without the two guarantees the Keychain item carries: it
    // would ride along in an encrypted backup, and it would not be pinned to
    // this phone. Saying that the write failed is better than quietly keeping
    // the seed somewhere weaker than the wallet claims to keep it.
    if cfg!(target_os = "ios") {
        return Err(VaultError::Storage);
    }

    let path = file(app)?;
    if let Some(parent) = path.parent() {
        // Nothing else creates this: `Library/Application Support` does not
        // exist in a fresh iOS container, and Tauri only resolves the path.
        fs::create_dir_all(parent).map_err(|_| VaultError::Storage)?;
    }

    // Written beside itself and moved into place — in the same directory, so the
    // move is a rename and not a copy across filesystems. A wallet interrupted
    // halfway through a PIN change is left with the record it had rather than
    // half of a new one.
    // **On Android, sealed under the Keystore's key** (`almena-keystore`): the
    // record is already encrypted under the PIN, but a copy of it taken off the
    // phone could be tried against every PIN at leisure, and its count of wrong
    // ones edited back to zero. Under a key that never leaves the phone's
    // secure hardware, it is nothing anywhere else. Should the Keystore refuse,
    // the file is written as before and the Security screen says so.
    #[cfg(target_os = "android")]
    let (written, home) = match almena_keystore::seal(app, bytes) {
        Ok(sealed) => ([SEALED, &sealed].concat(), Home::Keystore),
        Err(_) => (bytes.to_vec(), Home::File),
    };
    #[cfg(not(target_os = "android"))]
    let (written, home) = (bytes.to_vec(), Home::File);

    let temporary = path.with_extension("json.writing");
    let mut handle = create_private(&temporary).map_err(|_| VaultError::Storage)?;
    handle
        .write_all(&written)
        .map_err(|_| VaultError::Storage)?;
    handle.sync_all().map_err(|_| VaultError::Storage)?;
    drop(handle);
    fs::rename(&temporary, &path).map_err(|_| VaultError::Storage)?;

    // The store would not take it, so whatever it is still holding is older than
    // what was just written — and reading prefers the store. Left there, it
    // would come back the moment the store started answering again and quietly
    // undo this write.
    if let Ok(entry) = record_entry() {
        let _ = entry.delete_credential();
    }

    Ok(home)
}

/// Creates `path` for writing, readable by this user alone (`0600` where the
/// system has modes). A file left at `path` by an interrupted write is removed
/// first: truncating it would keep whatever mode it was created with.
pub(crate) fn create_private(path: &Path) -> std::io::Result<fs::File> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Removes the record and the device key, wherever either of them is.
pub fn clear<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Ok(entry) = record_entry() {
        let _ = entry.delete_credential();
    }
    if let Ok(entry) = device_entry() {
        let _ = entry.delete_credential();
    }
    if let Ok(path) = file(app) {
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json.writing"));
    }
}

/// The copy of the data key the platform holds, for a wallet that opens with a
/// face rather than digits.
///
/// **On iOS this is where the face is asked for.** The item is written with
/// `require-user-presence`, so the system stops this call until somebody has
/// been recognised. Elsewhere the prompt is the interface's to raise, and this
/// only hands back what was stored.
///
/// # Errors
///
/// [`VaultError::NoDeviceKey`] when nothing is there, and
/// [`VaultError::DeviceRefused`] when something is and the system would not
/// hand it over — a face not recognised, a prompt dismissed. The two are told
/// apart because only the first means the key is gone.
pub fn device_key() -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let entry = device_entry().map_err(|_| VaultError::NoDeviceKey)?;
    match entry.get_secret() {
        Ok(bytes) => Ok(Zeroizing::new(bytes)),
        Err(keyring_core::Error::NoEntry) => Err(VaultError::NoDeviceKey),
        Err(_) => Err(VaultError::DeviceRefused),
    }
}

/// Whether the device's own lock can be the wallet's, with no PIN beside it.
///
/// An iPhone, and only an iPhone: whether it has a passcode is answered by
/// [`set_device_lock`] trying, since the item it writes cannot exist without one.
pub const fn device_lockable() -> bool {
    cfg!(target_os = "ios")
}

/// Puts the data key where the iPhone's own lock is the only way to it.
///
/// `kSecAccessControlUserPresence` — Face ID or Touch ID first, the passcode
/// when that fails — over `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`:
/// never in a backup, never on another phone, and removed by the system if the
/// passcode is. Whatever copy was there before goes first, because writing over
/// an existing item changes its contents and keeps its old protection.
///
/// # Errors
///
/// [`VaultError::NoPasscode`] when the system will not take it, which on a phone
/// the wallet runs on means there is no passcode to put it behind.
#[cfg(target_os = "ios")]
pub fn set_device_lock(key: &[u8]) -> Result<(), VaultError> {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{
        set_generic_password_options, AccessControlOptions, PasswordOptions,
    };

    set_device_key(None)?;

    let access = SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
        AccessControlOptions::USER_PRESENCE.bits(),
    )
    .map_err(|_| VaultError::NoPasscode)?;

    // The same service and account the store reads it back under.
    let mut options = PasswordOptions::new_generic_password(SERVICE, DEVICE_KEY);
    options.use_protected_keychain();
    options.set_access_control(access);
    set_generic_password_options(key, options).map_err(|_| VaultError::NoPasscode)
}

/// Nowhere but an iPhone: see [`device_lockable`].
#[cfg(not(target_os = "ios"))]
pub fn set_device_lock(_key: &[u8]) -> Result<(), VaultError> {
    Err(VaultError::NoDeviceStore)
}

/// Puts the data key where the platform can hand it back, or takes it away.
///
/// # Errors
///
/// [`VaultError::NoDeviceStore`] where there is nothing to put it in.
pub fn set_device_key(key: Option<&[u8]>) -> Result<(), VaultError> {
    let entry = device_entry().map_err(|_| VaultError::NoDeviceStore)?;

    match key {
        Some(bytes) => entry
            .set_secret(bytes)
            .map_err(|_| VaultError::NoDeviceStore),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(_) => Err(VaultError::NoDeviceStore),
        },
    }
}

/// What the store holds, and the difference between empty and unavailable.
fn stored_record() -> Result<Option<Vec<u8>>, VaultError> {
    let entry = match record_entry() {
        Ok(entry) => entry,
        // No store was registered for this platform at all, which is Android and
        // is not a failure: the file is where the record lives there.
        Err(keyring_core::Error::NoDefaultStore) => return Ok(None),
        Err(_) => return Err(VaultError::Storage),
    };

    match entry.get_secret() {
        Ok(bytes) => Ok(Some(bytes)),
        Err(keyring_core::Error::NoEntry) => Ok(None),
        Err(_) => Err(VaultError::Storage),
    }
}

/// The record's entry, kept for this device alone.
fn record_entry() -> keyring_core::Result<keyring_core::Entry> {
    #[cfg(target_os = "ios")]
    {
        keyring_core::Entry::new_with_modifiers(
            SERVICE,
            RECORD,
            &std::collections::HashMap::from([("access-policy", "when-unlocked-this-device-only")]),
        )
    }

    #[cfg(not(target_os = "ios"))]
    keyring_core::Entry::new(SERVICE, RECORD)
}

/// The device key's entry, which the system will not hand over until it has seen
/// who is asking.
///
/// On iOS that is the default store, which is already the protected one. On
/// macOS it is the store held aside in [`init`], because the default there is
/// the file keychain and an item in it would be handed over to anybody. Where
/// neither applies there is no such entry, and saying so is
/// `NoDefaultStore` — the same answer a platform with no store at all gives.
fn device_entry() -> keyring_core::Result<keyring_core::Entry> {
    #[cfg(target_os = "ios")]
    {
        keyring_core::Entry::new_with_modifiers(SERVICE, DEVICE_KEY, &presence())
    }

    #[cfg(target_os = "macos")]
    {
        PROTECTED
            .get()
            .and_then(Option::as_ref)
            .ok_or(keyring_core::Error::NoDefaultStore)?
            .build(SERVICE, DEVICE_KEY, Some(&presence()))
    }

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    Err(keyring_core::Error::NoDefaultStore)
}

fn file<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, VaultError> {
    app.path()
        .app_local_data_dir()
        .map(|dir| dir.join(FILE))
        .map_err(|_| VaultError::Storage)
}
