//! Keeping the identity between launches.
//!
//! **This is what makes it a wallet.** Before it existed the seed lived in this
//! process and died with it, so every cold start asked for twelve words again —
//! which taught people to keep the phrase somewhere quick to reach, the one
//! place it must not be.
//!
//! What is written down is the seed and nothing else. The twelve words are the
//! backup and the backup belongs to the person: a wallet that held them would
//! hold something that can be read aloud, photographed off a screen or typed
//! into somebody else's wallet, and would make the sentence every warning in the
//! product rests on untrue. The seed derives every key this wallet will ever
//! sign with, so keeping it is enough, and it cannot be turned back into words.
//!
//! Two things open the record and neither of them is the record: a PIN, and —
//! where the platform has somewhere to keep a key — the device. Either is
//! enough, which is what lets a sensor that stops recognising somebody not be
//! the end of an identity, and a forgotten PIN not be either. The format is in
//! [`record`]; where it is kept is [`store`].
//!
//! **The lock lives here now.** It used to be a PIN hash in memory beside the
//! seed; there is nothing left to hash, because a PIN is right if and only if
//! what it derives opens the record. Nothing that verifies a PIN is stored, so
//! there is nothing to steal and compare against, and the check is a message
//! authentication code rather than a comparison.
//!
//! **Every PIN goes through one door.** There is exactly one function that
//! checks digits, and it is the one that spends an attempt — because a check
//! that does not count is a wallet that can be guessed at forever through
//! whichever screen forgot to count. All of it happens under one lock, so two
//! answers arriving at once cannot both start from nine attempts left.

mod record;
mod store;

use std::sync::Mutex;

use serde::{Serialize, Serializer};
use tauri::{Manager, Runtime, State};
use zeroize::Zeroizing;

use crate::identity::{Held, Identity};
use record::Record;

/// The two lengths a PIN can have.
pub(super) const LENGTHS: [u8; 2] = [4, 6];

/// Serialises everything that reads the record and writes it back.
///
/// The attempt count is read, changed and stored, and two commands doing that at
/// once from the same starting value would hand back an attempt that was already
/// spent. Nothing here is held across an await; these are ordinary blocking
/// commands run off the interface thread.
#[derive(Default)]
pub struct Gate(Mutex<()>);

/// What can go wrong, as codes rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum VaultError {
    /// The digits do not open it.
    WrongPin,
    /// The last attempt was spent, and the record is gone.
    Destroyed,
    /// Asked to open a wallet that has nothing written down.
    Nothing,
    /// Asked to write one down where there already is one.
    Exists,
    /// A PIN of a length this wallet does not offer.
    PinLength,
    /// Something other than digits.
    PinNotDigits,
    /// Asked to write down an identity while none is open.
    NoIdentity,
    /// Asked to open with the device, and the device holds no key.
    NoDeviceKey,
    /// The device would not hand back the key it had just been given, so
    /// whoever asked to arm it was not recognised.
    DeviceUnproven,
    /// This platform has nowhere to keep a device key.
    NoDeviceStore,
    /// What was found is not a record this wallet reads.
    Unreadable,
    /// A record a later version of the wallet wrote.
    TooNew,
    /// Somewhere that should have answered did not.
    Storage,
    /// The system would not supply randomness.
    Entropy,
}

impl VaultError {
    const fn code(self) -> &'static str {
        match self {
            Self::WrongPin => "vault_wrong_pin",
            Self::Destroyed => "vault_destroyed",
            Self::Nothing => "vault_nothing",
            Self::Exists => "vault_exists",
            Self::PinLength => "vault_pin_length",
            Self::PinNotDigits => "vault_pin_not_digits",
            Self::NoIdentity => "vault_no_identity",
            Self::NoDeviceKey => "vault_no_device_key",
            Self::DeviceUnproven => "vault_device_unproven",
            Self::NoDeviceStore => "vault_no_device_store",
            Self::Unreadable => "vault_unreadable",
            Self::TooNew => "vault_too_new",
            Self::Storage => "vault_storage",
            Self::Entropy => "vault_entropy",
        }
    }
}

impl Serialize for VaultError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// What the interface needs to know before it draws anything.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    /// Whether this device is holding an identity it can open.
    pub exists: bool,
    /// **Why it cannot be read**, when something is there and unreadable.
    ///
    /// The difference between "there is no identity here" and "there is one and
    /// this wallet cannot get at it" is the difference between offering somebody
    /// the way in and offering to create a second identity over the top of the
    /// one they have. A store that did not answer must never look like an empty
    /// device.
    pub problem: Option<&'static str>,
    /// How long the PIN is, so the keypad can be drawn before it is typed.
    pub digits: Option<u8>,
    /// Whether the device is holding a key, and so whether a face opens this.
    pub device_key: bool,
    /// Whether this platform can offer opening with the device at all.
    pub device_unlock: bool,
    /// Wrong PINs left before the record is destroyed.
    pub attempts_left: u8,
    /// Where the record is kept, which the Development section shows.
    pub home: Option<&'static str>,
    /// The format version found on the device.
    pub version: Option<u32>,
}

impl VaultStatus {
    /// What an empty device looks like, and what a device that will not answer
    /// looks like — which are told apart by `problem` and nothing else.
    const fn nothing(problem: Option<&'static str>) -> Self {
        Self {
            exists: false,
            problem,
            digits: None,
            device_key: false,
            device_unlock: false,
            attempts_left: record::ATTEMPTS,
            home: None,
            version: None,
        }
    }
}

/// Reads what is on the device, without opening it and without asking for a face.
///
/// **It does not touch the device key.** That item is stored so that the system
/// will not release it until somebody has been recognised, so reading it here
/// would raise Face ID on every launch before the lock screen was even drawn —
/// and a prompt somebody cancelled would then be indistinguishable from a wallet
/// that was never armed. The record's own note of what it armed is the answer.
#[tauri::command(async)]
pub fn vault_status<R: Runtime>(app: tauri::AppHandle<R>, gate: State<'_, Gate>) -> VaultStatus {
    let _held = gate.0.lock();

    status(&app)
}

/// Writes down the identity that is open, behind a PIN.
///
/// Called at the end of creating or restoring one: the seed is already held, and
/// this is what makes it survive the process.
///
/// # Errors
///
/// [`VaultError::NoIdentity`] with nothing open, [`VaultError::Exists`] where
/// there already is a record — including one that cannot be read, which must not
/// be written over — and the PIN errors for digits this wallet will not take.
#[tauri::command(async)]
pub fn vault_create<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    pin: String,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();
    let digits = check(&pin)?;

    // Anything at all where the record goes, readable or not, means there is
    // already an identity on this device.
    match load(&app) {
        Ok(None) => {}
        Ok(Some(_)) => return Err(VaultError::Exists),
        Err(VaultError::Unreadable | VaultError::TooNew) => return Err(VaultError::Exists),
        Err(failure) => return Err(failure),
    }

    let seed = held.seed().ok_or(VaultError::NoIdentity)?;
    let (record, _key) = Record::seal(&seed, &pin, digits)?;
    store::write(&app, &record.write())?;

    Ok(status(&app))
}

/// Opens the wallet with the PIN, and puts the identity back in front of it.
///
/// # Errors
///
/// [`VaultError::WrongPin`] for digits that do not open it, and
/// [`VaultError::Destroyed`] when that was the last attempt there was.
#[tauri::command(async)]
pub fn vault_open<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    pin: String,
) -> Result<Identity, VaultError> {
    let _guard = gate.0.lock();
    let (record, key) = attempt(&app, &held, &pin)?;

    Ok(crate::identity::adopt(record.seed(&key)?, &held))
}

/// Opens it with the key the device is holding, which the system will not
/// release until it has recognised somebody.
///
/// **The prompt is the lock, not a screen in front of it.** On iOS the item is
/// stored with `require-user-presence`, so this call is where the face is asked
/// for — see [`store`].
///
/// # Errors
///
/// [`VaultError::NoDeviceKey`] when the device is holding nothing, which is also
/// what a prompt somebody refused looks like from here.
#[tauri::command(async)]
pub fn vault_open_with_device<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> Result<Identity, VaultError> {
    let _guard = gate.0.lock();
    let (record, _) = load(&app)?.ok_or(VaultError::Nothing)?;
    let key = store::device_key().ok_or(VaultError::NoDeviceKey)?;
    let key: [u8; 32] = key
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::NoDeviceKey)?;

    Ok(crate::identity::adopt(record.seed(&key)?, &held))
}

/// Changes the PIN, leaving the identity exactly as it was.
///
/// # Errors
///
/// [`VaultError::WrongPin`] when the current one is not right — asked for so
/// that a wallet left open on a table cannot have its PIN quietly replaced, and
/// counted like every other answer.
#[tauri::command(async)]
pub fn vault_change_pin<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    current: String,
    next: String,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();
    let digits = check(&next)?;

    let (record, key) = attempt(&app, &held, &current)?;
    let changed = record.rewrap(&key, &next, digits)?;
    store::write(&app, &changed.write())?;

    Ok(status(&app))
}

/// Arms or disarms opening with the device.
///
/// Arming needs the PIN, because arming means handing the platform a key that
/// opens the wallet — a wallet somebody left unlocked on a table must not be
/// able to have a second way in added to it.
///
/// # Errors
///
/// [`VaultError::NoDeviceStore`] where the platform has nowhere to keep a key,
/// and [`VaultError::WrongPin`] when the digits given do not open the record.
#[tauri::command(async)]
pub fn vault_set_device<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    enabled: bool,
    pin: Option<String>,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();

    if enabled && !device_unlock() {
        return Err(VaultError::NoDeviceStore);
    }

    let mut record = if enabled {
        // Through the same door as every other PIN, so this screen cannot be
        // used to guess at one forever.
        let (record, key) = attempt(&app, &held, &pin.ok_or(VaultError::WrongPin)?)?;
        store::set_device_key(Some(&*key))?;

        // **Armed by proving it opens.** The key was just written behind the
        // system's own presence check, so asking for it back is what raises the
        // prompt — and answering it is the only evidence that the person turning
        // this on is the person it will let in. A wallet left unlocked on a desk
        // must not be able to have somebody else's face added to it.
        //
        // Anything other than the key coming back means it is not armed: a face
        // refused, a prompt dismissed, a sensor that would not answer. The key
        // goes rather than sitting there half-turned-on.
        match store::device_key() {
            Some(proof) if proof == *key.as_ref() => record,
            _ => {
                let _ = store::set_device_key(None);
                return Err(VaultError::DeviceUnproven);
            }
        }
    } else {
        store::set_device_key(None)?;
        load(&app)?.ok_or(VaultError::Nothing)?.0
    };

    record.device_wrap = enabled;
    store::write(&app, &record.write())?;

    Ok(status(&app))
}

/// Takes the identity off the device.
///
/// Signing out is this and nothing else: the record, the device key and the seed
/// in memory all go, and the twelve words are what is left.
#[tauri::command(async)]
pub fn vault_destroy<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
) -> VaultStatus {
    let _guard = gate.0.lock();

    store::clear(&app);
    held.forget();

    status(&app)
}

/// **The only place a PIN is ever checked.**
///
/// One answer, one attempt: the count goes up before the wallet says no, and the
/// record is destroyed rather than allowed an eleventh. A check that skipped this
/// would be a screen through which the PIN could be guessed at forever, so there
/// is nowhere else to check one.
fn attempt<R: Runtime>(
    app: &tauri::AppHandle<R>,
    held: &State<'_, Held>,
    pin: &str,
) -> Result<(Record, Zeroizing<[u8; 32]>), VaultError> {
    let (mut record, _) = load(app)?.ok_or(VaultError::Nothing)?;

    let key = match record.unwrap_pin(pin) {
        Ok(key) => key,
        Err(failure) => {
            record.wrong = record.wrong.saturating_add(1);
            if record.left() == 0 {
                // The way back is the phrase, which is the same way back as a
                // lost phone. Everything the device was holding goes, and so
                // does anything still open in front of it.
                store::clear(app);
                held.forget();
                return Err(VaultError::Destroyed);
            }
            store::write(app, &record.write())?;
            return Err(failure);
        }
    };

    // A count that only ever went up would eventually destroy a record whose
    // owner has been typing it correctly for a year.
    if record.wrong != 0 {
        record.wrong = 0;
        store::write(app, &record.write())?;
    }

    Ok((record, key))
}

/// Where the identity is being kept, in one word, for the line the log opens
/// with. "nothing" until there is one.
pub fn home_name<R: Runtime>(app: &tauri::AppHandle<R>) -> &'static str {
    load(app)
        .ok()
        .flatten()
        .map_or("nothing", |(_, home)| home.name())
}

/// Names the one secret store this platform has, before anything asks for it,
/// and registers the lock every command holds.
pub fn manage<R: Runtime>(app: &tauri::AppHandle<R>) {
    store::init();
    app.manage(Gate::default());
}

/// Whether the wallet may offer to open without the PIN on this platform.
///
/// **Only where the system enforces it.** On iOS the device key is stored so
/// that it is not handed back until somebody has been recognised, so the prompt
/// is the lock. On a computer the same keychain hands its items to whoever is
/// logged in, with no prompt at all — offering it there would be a second way in
/// that asks for nothing, which is not a convenience but the absence of a lock.
/// Android has no store to put the key in yet.
fn device_unlock() -> bool {
    cfg!(target_os = "ios") && store::has_store()
}

/// Whether the digits are ones this wallet takes, and how many there are.
fn check(pin: &str) -> Result<u8, VaultError> {
    let digits = u8::try_from(pin.chars().count()).map_err(|_| VaultError::PinLength)?;
    if !LENGTHS.contains(&digits) {
        return Err(VaultError::PinLength);
    }
    if !pin.chars().all(|character| character.is_ascii_digit()) {
        return Err(VaultError::PinNotDigits);
    }

    Ok(digits)
}

fn status<R: Runtime>(app: &tauri::AppHandle<R>) -> VaultStatus {
    match load(app) {
        Ok(Some((record, home))) => VaultStatus {
            exists: true,
            problem: None,
            digits: Some(record.digits),
            device_key: record.device_wrap,
            device_unlock: device_unlock(),
            attempts_left: record.left(),
            home: Some(home.name()),
            version: Some(record.version),
        },
        Ok(None) => VaultStatus::nothing(None),
        Err(failure) => VaultStatus::nothing(Some(failure.code())),
    }
}

fn load<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Option<(Record, store::Home)>, VaultError> {
    match store::read(app)? {
        Some((bytes, home)) => Ok(Some((Record::parse(&bytes)?, home))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_four_and_six_digits_are_taken() {
        assert_eq!(check("1234").expect("four digits"), 4);
        assert_eq!(check("123456").expect("six digits"), 6);

        assert!(matches!(check("123"), Err(VaultError::PinLength)));
        assert!(matches!(check("12345"), Err(VaultError::PinLength)));
        assert!(matches!(check(""), Err(VaultError::PinLength)));
        assert!(matches!(check("12a4"), Err(VaultError::PinNotDigits)));
        // Four characters, but not four digits anybody can type on the keypad.
        assert!(matches!(check("１２３４"), Err(VaultError::PinNotDigits)));
    }

    #[test]
    fn a_device_that_will_not_answer_is_not_a_device_with_no_identity() {
        // The distinction the welcome screen turns on: `exists` false with a
        // problem must never be read as an empty device, or somebody is offered
        // the chance to create a second identity over the one they have.
        let quiet = VaultStatus::nothing(None);
        assert!(!quiet.exists && quiet.problem.is_none());

        let broken = VaultStatus::nothing(Some(VaultError::Storage.code()));
        assert!(!broken.exists);
        assert_eq!(broken.problem, Some("vault_storage"));
    }
}
