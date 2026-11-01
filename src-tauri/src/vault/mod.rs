//! Keeping the identity between launches.
//!
//! **This is what makes it a wallet.** Before it existed the seed lived in this
//! process and died with it, so every cold start asked for the phrase again —
//! which taught people to keep the phrase somewhere quick to reach, the one
//! place it must not be.
//!
//! What is written down is the seed and nothing else. The words are the
//! backup and the backup belongs to the person: a wallet that held them would
//! hold something that can be read aloud, photographed off a screen or typed
//! into somebody else's wallet, and would make the sentence every warning in the
//! product rests on untrue. The seed derives every key this wallet will ever
//! sign with, so keeping it is enough, and it cannot be turned back into words.
//!
//! Two things open the record and neither of them is the record: a PIN, and —
//! where the system keeps a key behind its own presence check ([`presence`]) —
//! the device. Either is
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
//! **On an iPhone with a passcode there is no PIN at all.** The device's own
//! lock — the face first, the passcode after it — is the wallet's, and the
//! record carries no PIN wrap: see [`store::set_device_lock`]. A PIN is chosen
//! only where the iPhone has no passcode, and a wallet that has one is moved
//! over the next time it is opened, once the passcode is there.
//!
//! **Every PIN goes through one door.** There is exactly one function that
//! checks digits, and it is the one that spends an attempt — because a check
//! that does not count is a wallet that can be guessed at forever through
//! whichever screen forgot to count. All of it happens under one lock, so two
//! answers arriving at once cannot both start from nine attempts left.

mod presence;
mod record;
pub(crate) mod store;

use std::sync::Mutex;

use serde::{Serialize, Serializer};
use tauri::{Manager, Runtime, State};
use zeroize::Zeroizing;

use crate::identity::{Held, Identity};
use presence::Keeper as _;
use record::Record;

/// The two lengths a PIN can have.
pub(super) const LENGTHS: [u8; 2] = [4, 6];

/// **The shortest PIN a new or changed one may be, given where the record is
/// kept.**
///
/// Four digits are ten thousand guesses. Behind hardware that never lets the
/// record off the device ([`store::hardware_bound`]) only ten of them are ever
/// answered, so four are enough. Anywhere a process running as the same user
/// can copy the record, all ten thousand can be tried offline — Argon2id at
/// 64 MiB makes each one cost, not impossible — so a PIN is six digits there,
/// a hundred times the work. A record already holding four is opened as
/// before, and the interface then asks for a longer one (`lengthen`).
pub(super) const fn shortest(home: Option<store::Home>) -> u8 {
    if store::hardware_bound(home) {
        LENGTHS[0]
    } else {
        LENGTHS[1]
    }
}

/// What `vault_destroy` must be handed, so that nothing removes an identity by
/// calling it without meaning to — a stray `invoke`, a screen that forgot to
/// ask. The interface sends it only from the sign-out screen, once somebody
/// has confirmed there.
const DESTROY: &str = "sign-out";

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
    /// Shorter than this device lets a new PIN be — see [`shortest`].
    PinTooShort,
    /// Asked to remove the identity without the confirmation that says so.
    Unconfirmed,
    /// Asked to write down an identity while none is open.
    NoIdentity,
    /// Asked to open with the device, and the device holds no key.
    NoDeviceKey,
    /// The device would not hand back the key it had just been given, so
    /// whoever asked to arm it was not recognised.
    DeviceUnproven,
    /// This platform has nowhere to keep a device key.
    NoDeviceStore,
    /// The device would not hand the key back: a face not recognised, a prompt
    /// dismissed.
    DeviceRefused,
    /// The device's copy can no longer come back — a fingerprint or face
    /// enrolled since it was armed, Windows Hello reset — so it no longer opens
    /// the wallet and has been turned off. The PIN still does.
    #[cfg_attr(not(any(target_os = "android", windows, test)), allow(dead_code))]
    DeviceChanged,
    /// Asked for digits on a record that has none.
    NoPin,
    /// The iPhone has no passcode, so its lock cannot be the wallet's.
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    NoPasscode,
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
            Self::PinTooShort => "vault_pin_too_short",
            Self::Unconfirmed => "vault_unconfirmed",
            Self::NoIdentity => "vault_no_identity",
            Self::NoDeviceKey => "vault_no_device_key",
            Self::DeviceUnproven => "vault_device_unproven",
            Self::NoDeviceStore => "vault_no_device_store",
            Self::DeviceRefused => "vault_device_refused",
            Self::DeviceChanged => "vault_device_changed",
            Self::NoPin => "vault_no_pin",
            Self::NoPasscode => "vault_no_passcode",
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
    /// Whether digits open this at all. False where the device's lock is the
    /// only one.
    pub pin: bool,
    /// How long the PIN is, so the keypad can be drawn before it is typed.
    pub digits: Option<u8>,
    /// The shortest PIN this device takes when one is chosen or changed.
    pub min_digits: u8,
    /// **Whether the PIN is shorter than this device now asks for**: a wallet
    /// that chose four digits where the record can be copied off. It still
    /// opens; once it has, the interface asks for a longer one and does not let
    /// the wallet be used until there is.
    pub lengthen: bool,
    /// Whether the device's own lock can be the wallet's, with no PIN: an iPhone.
    pub device_lock: bool,
    /// Whether the device is holding a key, and so whether a face opens this.
    pub device_key: bool,
    /// Whether this platform can offer opening with the device at all.
    pub device_unlock: bool,
    /// **Whether the device's copy can no longer come back**: armed, and since
    /// then a fingerprint or face was enrolled, or Windows Hello was reset.
    /// `device_key` is then false; the record is disarmed at the next opening.
    pub device_changed: bool,
    /// Which system recognises the person here, so the interface names it as
    /// that system does: `apple`, `android`, `windows`; none on Linux.
    pub biometrics: Option<&'static str>,
    /// Wrong PINs left before the record is destroyed.
    pub attempts_left: u8,
    /// Where the record is kept: the platform's secret store or a private file.
    pub home: Option<&'static str>,
    /// The format version found on the device.
    pub version: Option<u32>,
}

impl VaultStatus {
    /// What an empty device looks like, and what a device that will not answer
    /// looks like — which are told apart by `problem` and nothing else.
    fn nothing(problem: Option<&'static str>) -> Self {
        Self {
            exists: false,
            problem,
            pin: false,
            digits: None,
            min_digits: shortest(None),
            lengthen: false,
            device_lock: store::device_lockable(),
            device_key: false,
            device_unlock: false,
            device_changed: false,
            biometrics: presence::KIND.map(presence::Kind::name),
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
/// that was never armed. The record's own note of what it armed is the answer,
/// and on Android and Windows the system is asked only whether that copy can
/// still come back ([`presence::Keeper::changed`]), which shows nothing.
#[tauri::command(async)]
pub fn vault_status<R: Runtime>(app: tauri::AppHandle<R>, gate: State<'_, Gate>) -> VaultStatus {
    let _held = gate.0.lock();

    status(&app)
}

/// Writes down the identity that is open, behind a PIN or behind the device.
///
/// Called at the end of creating or restoring one: the seed is already held, and
/// this is what makes it survive the process. With no PIN the device's own lock
/// is the only one, which only an iPhone with a passcode offers.
///
/// # Errors
///
/// [`VaultError::NoIdentity`] with nothing open, [`VaultError::Exists`] where
/// there already is a record — including one that cannot be read, which must not
/// be written over — the PIN errors for digits this wallet will not take, and
/// [`VaultError::NoPasscode`] with no PIN on an iPhone that has no passcode,
/// which is the interface's cue to ask for one.
#[tauri::command(async)]
pub fn vault_create<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    pin: Option<Zeroizing<String>>,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();
    let digits = pin
        .as_deref()
        .map(|pin| check(pin, shortest(None)))
        .transpose()?;
    if pin.is_none() && !store::device_lockable() {
        return Err(VaultError::NoDeviceStore);
    }

    // Anything at all where the record goes, readable or not, means there is
    // already an identity on this device.
    match load(&app) {
        Ok(None) => {}
        Ok(Some(_)) => return Err(VaultError::Exists),
        Err(VaultError::Unreadable | VaultError::TooNew) => return Err(VaultError::Exists),
        Err(failure) => return Err(failure),
    }

    let seed = held.seed().ok_or(VaultError::NoIdentity)?;
    let record = match (pin, digits) {
        (Some(pin), Some(digits)) => Record::seal(&seed, &pin, digits)?.0,
        _ => {
            // The key first: a record written without a PIN and without the
            // key beside it would be one nothing opens.
            let (record, key) = Record::seal_device(&seed)?;
            store::set_device_lock(&*key)?;
            record
        }
    };
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
    pin: Zeroizing<String>,
) -> Result<Identity, VaultError> {
    let _guard = gate.0.lock();
    let (record, key) = attempt(&app, &held, &pin)?;
    let seed = record.seed(&key)?;
    to_device_lock(&app, &record, &key);

    // A copy the system can no longer hand back is turned off now that the PIN
    // has opened it, so the lock screen stops offering it.
    let keeper = presence::system(&app, None);
    if presence::changed(&keeper, &record) {
        if let Ok(disarmed) = presence::disarm(&keeper, record) {
            let _ = store::write(&app, &disarmed.write());
        }
    }

    Ok(crate::identity::adopt(seed, &held))
}

/// Opens it with the key the device is holding, which the system will not
/// release until it has recognised somebody.
///
/// **The prompt is the lock, not a screen in front of it.** On iOS the item is
/// stored with `require-user-presence`, so this call is where the face is asked
/// for — see [`store`]; on Android it is a `BiometricPrompt` unlocking the
/// Keystore cipher, on Windows the Hello signature ([`presence`]). `prompt` is
/// the words Android's prompt shows.
///
/// # Errors
///
/// [`VaultError::NoDeviceKey`] when the device is holding nothing, which is also
/// what a prompt somebody refused looks like from here, and
/// [`VaultError::DeviceChanged`] when its copy can no longer come back — the
/// record is then written down disarmed, and the PIN is the way in.
#[tauri::command(async)]
pub fn vault_open_with_device<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    prompt: Option<presence::Prompt>,
) -> Result<Identity, VaultError> {
    let _guard = gate.0.lock();
    let (record, _) = load(&app)?.ok_or(VaultError::Nothing)?;
    let key = match presence::open(&presence::system(&app, prompt), &record)? {
        presence::Opened::Key(key) => key,
        presence::Opened::Changed(disarmed) => {
            store::write(&app, &disarmed.write())?;
            return Err(VaultError::DeviceChanged);
        }
    };
    let seed = record.seed(&key)?;
    to_device_lock(&app, &record, &key);

    Ok(crate::identity::adopt(seed, &held))
}

/// **Moves a wallet with a PIN over to the iPhone's lock**, once it can be.
///
/// Called with the record just opened and the key that opened it. Where the
/// device's lock cannot be the wallet's — not an iPhone, or one still without a
/// passcode — nothing changes and the PIN stays; it is tried again at the next
/// opening. The key goes in first and the PIN comes off after, so a failure
/// between the two leaves a record the PIN still opens.
fn to_device_lock<R: Runtime>(app: &tauri::AppHandle<R>, record: &Record, key: &[u8; 32]) {
    if !record.has_pin() || !store::device_lockable() {
        return;
    }
    if store::set_device_lock(key).is_ok() {
        let _ = store::write(app, &record.without_pin().write());
    }
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
    current: Zeroizing<String>,
    next: Zeroizing<String>,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();
    let home = load(&app)?.map(|(_, home)| home);
    let digits = check(&next, shortest(home))?;

    let (record, key) = attempt(&app, &held, &current)?;
    let changed = record.rewrap(&key, &next, digits)?;
    store::write(&app, &changed.write())?;

    Ok(status(&app))
}

/// Arms or disarms opening with the device.
///
/// Arming needs the PIN, because arming means handing the platform a key that
/// opens the wallet — a wallet somebody left unlocked on a table must not be
/// able to have a second way in added to it. How it is armed and proven is
/// [`presence::arm`], the same on every platform; `prompt` is the words
/// Android's prompt shows.
///
/// # Errors
///
/// [`VaultError::NoDeviceStore`] where the platform has nowhere to keep a key,
/// [`VaultError::WrongPin`] when the digits given do not open the record, and
/// [`VaultError::DeviceUnproven`] when nobody was recognised.
#[tauri::command(async)]
pub fn vault_set_device<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    enabled: bool,
    pin: Option<Zeroizing<String>>,
    prompt: Option<presence::Prompt>,
) -> Result<VaultStatus, VaultError> {
    let _guard = gate.0.lock();
    let keeper = presence::system(&app, prompt);

    // Where the device's lock is the only one, taking its key away would leave
    // nothing that opens the record.
    if !load(&app)?.ok_or(VaultError::Nothing)?.0.has_pin() {
        return Err(VaultError::NoPin);
    }
    if enabled && !keeper.available() {
        return Err(VaultError::NoDeviceStore);
    }

    let record = if enabled {
        // Through the same door as every other PIN, so this screen cannot be
        // used to guess at one forever.
        let (record, key) = attempt(&app, &held, &pin.ok_or(VaultError::WrongPin)?)?;
        // **Armed by proving it opens**: answering the system's own prompt is
        // the only evidence that the person turning this on is the person it
        // will let in. A wallet left unlocked on a desk must not be able to
        // have somebody else's face added to it.
        presence::arm(&keeper, record, &key)?
    } else {
        presence::disarm(&keeper, load(&app)?.ok_or(VaultError::Nothing)?.0)?
    };

    store::write(&app, &record.write())?;

    Ok(status(&app))
}

/// Confirms that the person in front of the open wallet is its holder, before
/// something that asks for more than an unlocked wallet: checking the recovery
/// phrase (`identity::identity_check_phrase`), for a few minutes from now.
///
/// With `pin`, through [`attempt`] like every other PIN, so it is counted.
/// Without, the device's own check — biometrics, or an iPhone's Face ID and
/// passcode — and the key it hands back must open the record. `prompt` is the
/// words Android's prompt shows.
///
/// # Errors
///
/// [`VaultError::WrongPin`] (or [`VaultError::Destroyed`]) for digits that do
/// not open the record, [`VaultError::NoDeviceKey`] or
/// [`VaultError::DeviceRefused`] when the device did not recognise anybody, and
/// [`VaultError::DeviceChanged`] when its copy can no longer come back.
#[tauri::command(async)]
pub fn vault_confirm<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    pin: Option<Zeroizing<String>>,
    prompt: Option<presence::Prompt>,
) -> Result<(), VaultError> {
    let _guard = gate.0.lock();
    if let Some(pin) = pin {
        attempt(&app, &held, &pin)?;
    } else {
        let (record, _) = load(&app)?.ok_or(VaultError::Nothing)?;
        let key = match presence::open(&presence::system(&app, prompt), &record)? {
            presence::Opened::Key(key) => key,
            presence::Opened::Changed(disarmed) => {
                store::write(&app, &disarmed.write())?;
                return Err(VaultError::DeviceChanged);
            }
        };
        // A key that does not open the record proves nothing.
        record.seed(&key)?;
    }
    held.confirm();
    Ok(())
}

/// Takes the identity off the device.
///
/// Signing out is this and nothing else: the record, the device key, what
/// messaging wrote down and the seed in memory all go, and the phrase is what
/// is left. **It asks to be told so** ([`DESTROY`]): it cannot be undone, and
/// a call that does not say what it is for removes nothing.
///
/// # Errors
///
/// [`VaultError::Unconfirmed`] when `confirm` is not [`DESTROY`].
#[tauri::command(async)]
pub fn vault_destroy<R: Runtime>(
    app: tauri::AppHandle<R>,
    held: State<'_, Held>,
    gate: State<'_, Gate>,
    confirm: Option<String>,
) -> Result<VaultStatus, VaultError> {
    confirmed(confirm.as_deref())?;
    let _guard = gate.0.lock();

    store::clear(&app);
    crate::messaging::clear(&app);
    held.forget();

    Ok(status(&app))
}

/// Whether `vault_destroy` was told what it is for.
fn confirmed(confirm: Option<&str>) -> Result<(), VaultError> {
    if confirm == Some(DESTROY) {
        Ok(())
    } else {
        Err(VaultError::Unconfirmed)
    }
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
    let (record, _) = load(app)?.ok_or(VaultError::Nothing)?;
    counted(record, pin, |record| {
        store::write(app, &record.write()).map(|_| ())
    })
    .inspect_err(|failure| {
        if matches!(failure, VaultError::Destroyed) {
            destroy(app, held);
        }
    })
}

/// The count itself, given how to write the record down: [`attempt`] without
/// the device, so it can be tested. [`VaultError::Destroyed`] is the caller's
/// cue to remove everything.
fn counted(
    mut record: Record,
    pin: &str,
    mut persist: impl FnMut(&Record) -> Result<(), VaultError>,
) -> Result<(Record, Zeroizing<[u8; 32]>), VaultError> {
    // Not an answer, so not an attempt: there are no digits to get wrong.
    if !record.has_pin() {
        return Err(VaultError::NoPin);
    }

    // Counted before it is tried, and written down before the slow part starts.
    // Counting it only once Argon2 has said no would let whoever kills the
    // wallet during the derivation — or makes the write fail — guess for free.
    // A record that reaches here already spent is one whose last guess was
    // never answered: it goes the way it would have gone.
    if record.left() == 0 {
        return Err(VaultError::Destroyed);
    }
    record.wrong = record.wrong.saturating_add(1);
    persist(&record)?;

    let key = match record.unwrap_pin(pin) {
        Ok(key) => key,
        Err(_) if record.left() == 0 => return Err(VaultError::Destroyed),
        Err(failure) => return Err(failure),
    };

    // A count that only ever went up would eventually destroy a record whose
    // owner has been typing it correctly for a year.
    record.wrong = 0;
    persist(&record)?;

    Ok((record, key))
}

/// The way back is the phrase, which is the same way back as a lost phone.
/// Everything the device was holding goes, and so does anything still open in
/// front of it.
fn destroy<R: Runtime>(app: &tauri::AppHandle<R>, held: &State<'_, Held>) {
    store::clear(app);
    crate::messaging::clear(app);
    held.forget();
}

/// Names the one secret store this platform has, before anything asks for it,
/// and registers the lock every command holds.
pub fn manage<R: Runtime>(app: &tauri::AppHandle<R>) {
    store::init();
    app.manage(Gate::default());
}

/// Whether the digits are ones this wallet takes for a new PIN, at least
/// `shortest` of them, and how many there are.
fn check(pin: &str, shortest: u8) -> Result<u8, VaultError> {
    let digits = u8::try_from(pin.chars().count()).map_err(|_| VaultError::PinLength)?;
    if !LENGTHS.contains(&digits) {
        return Err(VaultError::PinLength);
    }
    if !pin.chars().all(|character| character.is_ascii_digit()) {
        return Err(VaultError::PinNotDigits);
    }
    if digits < shortest {
        return Err(VaultError::PinTooShort);
    }

    Ok(digits)
}

/// Whether a record's PIN is shorter than one chosen where it is kept may be.
fn too_short(record: &Record, home: store::Home) -> bool {
    record.has_pin() && record.digits.unwrap_or(0) < shortest(Some(home))
}

/// **Whether the wallet may offer to open without the PIN** is the keeper's
/// [`presence::Keeper::available`]: only where the system enforces it, and only
/// where the machine can actually recognise a person — see [`presence`]. Where
/// it is false the PIN is the only way in, and the interface says so.
fn status<R: Runtime>(app: &tauri::AppHandle<R>) -> VaultStatus {
    match load(app) {
        Ok(Some((record, home))) => {
            let keeper = presence::system(app, None);
            let changed = presence::changed(&keeper, &record);
            VaultStatus {
                exists: true,
                problem: None,
                pin: record.has_pin(),
                digits: record.digits,
                min_digits: shortest(Some(home)),
                lengthen: too_short(&record, home),
                device_lock: store::device_lockable(),
                device_key: record.device_wrap && !changed,
                device_unlock: keeper.available(),
                device_changed: changed,
                biometrics: presence::KIND.map(presence::Kind::name),
                attempts_left: record.left(),
                home: Some(home.name()),
                version: Some(record.version),
            }
        }
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
    use super::{counted, record::Record, record::ATTEMPTS, VaultError};

    #[test]
    fn every_answer_is_written_down_as_an_attempt_before_it_is_checked() {
        let seed = [7u8; 64];
        let (record, _) = Record::seal(&seed, "2468", 4).unwrap();
        let mut written: Vec<u8> = Vec::new();

        // A wrong PIN: counted, and written with the count before the answer.
        let failure = counted(record.clone(), "1111", |r| {
            written.push(r.wrong);
            Ok(())
        });
        assert!(matches!(failure, Err(VaultError::WrongPin)));
        assert_eq!(written, vec![1]);

        // A write that fails is no attempt at all: nothing is checked.
        let mut checked = false;
        let refused = counted(record.clone(), "2468", |_| {
            if checked {
                return Ok(());
            }
            checked = true;
            Err(VaultError::Storage)
        });
        assert!(matches!(refused, Err(VaultError::Storage)));

        // The right one: counted, then the count goes back to zero.
        let mut wrong = record.clone();
        wrong.wrong = 3;
        written.clear();
        let (opened, _) = counted(wrong, "2468", |r| {
            written.push(r.wrong);
            Ok(())
        })
        .unwrap();
        assert_eq!(written, vec![4, 0]);
        assert_eq!(opened.wrong, 0);
    }

    #[test]
    fn the_last_wrong_answer_and_a_spent_record_destroy_it() {
        let seed = [7u8; 64];
        let (mut record, _) = Record::seal(&seed, "2468", 4).unwrap();
        record.wrong = ATTEMPTS - 1;
        assert!(matches!(
            counted(record.clone(), "1111", |_| Ok(())),
            Err(VaultError::Destroyed)
        ));
        // Killed during its last answer: the count says it was spent.
        record.wrong = ATTEMPTS;
        assert!(matches!(
            counted(record, "2468", |_| Ok(())),
            Err(VaultError::Destroyed)
        ));
    }

    use super::*;

    #[test]
    fn only_four_and_six_digits_are_taken() {
        assert_eq!(check("1234", 4).expect("four digits"), 4);
        assert_eq!(check("123456", 4).expect("six digits"), 6);

        assert!(matches!(check("123", 4), Err(VaultError::PinLength)));
        assert!(matches!(check("12345", 4), Err(VaultError::PinLength)));
        assert!(matches!(check("", 4), Err(VaultError::PinLength)));
        assert!(matches!(check("12a4", 4), Err(VaultError::PinNotDigits)));
        // Four characters, but not four digits anybody can type on the keypad.
        assert!(matches!(
            check("１２３４", 4),
            Err(VaultError::PinNotDigits)
        ));
    }

    #[test]
    fn four_digits_are_refused_where_the_record_can_be_copied_off() {
        assert!(matches!(check("1234", 6), Err(VaultError::PinTooShort)));
        assert_eq!(check("123456", 6).expect("six digits"), 6);
        // Not digits is said before too short: it is the first thing wrong.
        assert!(matches!(check("12a4", 6), Err(VaultError::PinNotDigits)));
    }

    #[test]
    fn the_shortest_pin_follows_where_the_record_is_kept() {
        use store::Home;

        // A plain file is a file on every platform.
        assert_eq!(shortest(Some(Home::File)), 6);
        if cfg!(target_os = "ios") {
            assert_eq!(shortest(Some(Home::Store)), 4);
            assert_eq!(shortest(None), 4);
        } else if cfg!(target_os = "android") {
            assert_eq!(shortest(Some(Home::Keystore)), 4);
            assert_eq!(shortest(None), 4);
        } else {
            // A computer's secret store hands the record to its user.
            assert_eq!(shortest(Some(Home::Store)), 6);
            assert_eq!(shortest(None), 6);
        }
    }

    #[test]
    fn a_short_pin_is_asked_to_be_lengthened_only_where_it_is_too_short() {
        let seed = [7u8; 64];
        let (four, _) = Record::seal(&seed, "2468", 4).unwrap();
        let (six, _) = Record::seal(&seed, "246813", 6).unwrap();
        let (device, _) = Record::seal_device(&seed).unwrap();

        assert!(too_short(&four, store::Home::File));
        assert!(!too_short(&six, store::Home::File));
        // No PIN, nothing to lengthen.
        assert!(!too_short(&device, store::Home::File));
    }

    #[test]
    fn signing_out_has_to_be_meant() {
        assert!(confirmed(Some(DESTROY)).is_ok());
        assert!(matches!(confirmed(None), Err(VaultError::Unconfirmed)));
        assert!(matches!(
            confirmed(Some("yes")),
            Err(VaultError::Unconfirmed)
        ));
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
