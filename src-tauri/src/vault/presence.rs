//! Whether this machine can recognise the person in front of it, and the one
//! place a copy of the data key is kept behind that recognition.
//!
//! **Not the same question as whether there is somewhere to keep a key.** A Mac
//! mini with an ordinary keyboard has the data protection keychain and no
//! sensor at all, and `kSecAccessControlUserPresence` on such a machine falls
//! back to the login password. That is still a lock the system enforces — but a
//! switch labelled with a fingerprint that asks for a password is a switch that
//! lied, so it is not offered where there is no finger to read.
//!
//! Each platform keeps the key its own way, and the vault sees one [`Keeper`]:
//!
//! - **iOS and macOS** — the Keychain item behind `require-user-presence`
//!   ([`super::store`]). iOS is not asked whether it can recognise anybody:
//!   every device the wallet runs on there has Face ID, Touch ID or a passcode,
//!   one of the three is always set up by the time an identity exists, and the
//!   item's own access control is what enforces it.
//! - **Android** — a second Keystore key that only a strong biometric unlocks,
//!   for one operation at a time, and that the system invalidates when a
//!   fingerprint or face is enrolled ([`android`]).
//! - **Windows** — a Windows Hello key in the TPM, whose signature of a stored
//!   challenge derives the key that wraps the data key ([`hello`]).
//! - **Linux** — nothing. There is no standard presence API: fprintd and
//!   howdy are optional daemons any process of the user can talk to, PAM is
//!   for logging in, and neither puts a key behind the answer. The Secret
//!   Service hands its items to whoever is logged in, so a switch there would
//!   be a way in that asks for nothing.
//!
//! The rules every platform shares are here and tested once ([`arm`],
//! [`disarm`], [`open`], [`changed`]): arming costs the PIN and is proven by
//! the system's own check, and a key the system says can no longer come back
//! turns biometrics off rather than leaving a button that cannot work. The PIN
//! still opens the record either way.

#[cfg(target_os = "android")]
mod android;
// Compiled on every host so its derivation is tested everywhere; only Windows
// signs anything with it.
#[cfg_attr(not(windows), allow(dead_code))]
mod hello;

use serde::Deserialize;
use tauri::{AppHandle, Runtime};
use zeroize::Zeroizing;

use super::record::Record;
use super::VaultError;

/// The data key: what a PIN unwraps, and what the system keeps a copy of.
pub type Key = [u8; 32];

/// Which system does the recognising, so the interface can name it the way that
/// system does: Face ID or Touch ID, a fingerprint or a face, Windows Hello.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Apple,
    Android,
    Windows,
}

impl Kind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Apple => "apple",
            Self::Android => "android",
            Self::Windows => "windows",
        }
    }
}

/// This platform's, or none on Linux — see the module's notes.
pub const KIND: Option<Kind> = if cfg!(any(target_os = "ios", target_os = "macos")) {
    Some(Kind::Apple)
} else if cfg!(target_os = "android") {
    Some(Kind::Android)
} else if cfg!(windows) {
    Some(Kind::Windows)
} else {
    None
};

/// The words the system's prompt shows, in the interface's language.
///
/// Only Android draws its prompt from words the app gives it. Apple's prompt is
/// worded by the system and the bundle's `NSFaceIDUsageDescription`, and
/// Windows Hello's sign prompt takes no text at all.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub struct Prompt {
    pub title: String,
    pub subtitle: String,
    /// The button that closes the prompt and leaves the PIN: "Use PIN".
    pub cancel: String,
}

/// Whether putting the key away already answered the presence check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proof {
    /// It did: the key could not have been sealed without somebody being
    /// recognised (a Keystore cipher a biometric unlocked, a Hello signature).
    #[cfg_attr(not(any(target_os = "android", windows, test)), allow(dead_code))]
    Given,
    /// It did not: the Keychain takes an item without a prompt, so it has to be
    /// read back — which is what raises the prompt.
    #[cfg_attr(
        not(any(target_os = "ios", target_os = "macos", test)),
        allow(dead_code)
    )]
    Owed,
}

/// Somewhere that keeps a copy of the data key and will not hand it back until
/// the system has recognised somebody.
pub trait Keeper {
    /// Whether this device can do it at all, right now: a sensor with something
    /// enrolled, a store that will hold the key behind it.
    fn available(&self) -> bool;
    /// Keeps `key`, replacing whatever copy was there.
    ///
    /// # Errors
    ///
    /// [`VaultError::DeviceRefused`] or [`VaultError::DeviceUnproven`] when the
    /// person was not recognised, [`VaultError::NoDeviceStore`] when there is
    /// nowhere to keep it.
    fn put(&self, key: &Key) -> Result<Proof, VaultError>;
    /// The copy, once the system has recognised somebody.
    ///
    /// # Errors
    ///
    /// [`VaultError::NoDeviceKey`] when nothing is kept, [`VaultError::DeviceRefused`]
    /// when the person was not recognised or dismissed the prompt, and
    /// [`VaultError::DeviceChanged`] when what is kept can no longer come back.
    fn take(&self) -> Result<Zeroizing<Key>, VaultError>;
    /// Removes the copy. Not an error when there is none.
    ///
    /// # Errors
    ///
    /// [`VaultError::NoDeviceStore`] when the store would not say it is gone.
    fn remove(&self) -> Result<(), VaultError>;
    /// **Whether a kept copy can no longer come back** — a fingerprint enrolled
    /// since it was armed, Windows Hello reset — asked without a prompt.
    fn changed(&self) -> bool;
}

/// Hands `record`'s data key to the system, and gives the record back armed.
///
/// The caller has just opened the record with the PIN: arming means handing the
/// platform a second way in, and a wallet left unlocked on a table must not be
/// able to have one added to it by whoever picks it up.
///
/// **Armed by proving it opens.** Either sealing the key already needed the
/// person to be recognised ([`Proof::Given`]), or the key is asked for back,
/// which is what raises the prompt ([`Proof::Owed`]). Anything else — a face
/// refused, a prompt dismissed, a sensor that would not answer — leaves nothing
/// half-turned-on behind it.
///
/// # Errors
///
/// [`VaultError::NoDeviceStore`] where this device cannot do it,
/// [`VaultError::DeviceUnproven`] when nobody was recognised.
pub fn arm(keeper: &impl Keeper, mut record: Record, key: &Key) -> Result<Record, VaultError> {
    if !keeper.available() {
        return Err(VaultError::NoDeviceStore);
    }

    let proven = match keeper.put(key) {
        Ok(Proof::Given) => true,
        Ok(Proof::Owed) => keeper
            .take()
            .is_ok_and(|proof| proof.as_slice() == key.as_slice()),
        Err(failure) => {
            let _ = keeper.remove();
            return Err(match failure {
                VaultError::DeviceRefused => VaultError::DeviceUnproven,
                other => other,
            });
        }
    };
    if !proven {
        let _ = keeper.remove();
        return Err(VaultError::DeviceUnproven);
    }

    record.device_wrap = true;
    Ok(record)
}

/// Takes the system's copy away, and gives the record back without it.
///
/// # Errors
///
/// Whatever the keeper says when it would not remove the copy; the record is
/// then left armed, since the copy is still there.
pub fn disarm(keeper: &impl Keeper, mut record: Record) -> Result<Record, VaultError> {
    keeper.remove()?;
    record.device_wrap = false;
    Ok(record)
}

/// What opening with the device came to.
pub enum Opened {
    /// The data key, handed back by the system.
    Key(Zeroizing<Key>),
    /// The system's copy can no longer come back. It has been removed, and this
    /// is the record disarmed, for the caller to write down before saying so.
    Changed(Record),
}

/// The data key, from the system's copy.
///
/// # Errors
///
/// [`VaultError::NoDeviceKey`] when the record was never armed, and the
/// keeper's errors otherwise — except a change on a record with a PIN, which is
/// [`Opened::Changed`]: the PIN still opens it, so biometrics turn off and the
/// wallet says why. A record with no PIN (an iPhone's own lock) is never
/// disarmed here: nothing else would open it.
pub fn open(keeper: &impl Keeper, record: &Record) -> Result<Opened, VaultError> {
    if !record.device_wrap {
        return Err(VaultError::NoDeviceKey);
    }

    match keeper.take() {
        Ok(key) => Ok(Opened::Key(key)),
        Err(VaultError::DeviceChanged) if record.has_pin() => {
            let _ = keeper.remove();
            let mut disarmed = record.clone();
            disarmed.device_wrap = false;
            Ok(Opened::Changed(disarmed))
        }
        Err(failure) => Err(failure),
    }
}

/// Whether `record` is armed with a copy that can no longer come back, so the
/// lock screen offers the PIN alone and says why.
pub fn changed(keeper: &impl Keeper, record: &Record) -> bool {
    record.device_wrap && record.has_pin() && keeper.changed()
}

/// The keeper this platform has, asked from the vault's commands.
pub fn system<R: Runtime>(app: &AppHandle<R>, prompt: Option<Prompt>) -> System<'_, R> {
    System {
        app,
        prompt: prompt.unwrap_or_default(),
    }
}

/// See [`Keeper`]: one implementation, its body chosen per platform.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub struct System<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    prompt: Prompt,
}

impl<R: Runtime> Keeper for System<'_, R> {
    fn available(&self) -> bool {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            super::store::has_device_store() && sensor()
        }
        #[cfg(target_os = "android")]
        {
            android::available(self.app)
        }
        #[cfg(windows)]
        {
            hello::available()
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android", windows)))]
        {
            false
        }
    }

    fn put(&self, key: &Key) -> Result<Proof, VaultError> {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            super::store::set_device_key(Some(key)).map(|()| Proof::Owed)
        }
        #[cfg(target_os = "android")]
        {
            android::arm(self.app, key, &self.prompt).map(|()| Proof::Given)
        }
        #[cfg(windows)]
        {
            hello::arm(key).map(|()| Proof::Given)
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android", windows)))]
        {
            let _ = key;
            Err(VaultError::NoDeviceStore)
        }
    }

    fn take(&self) -> Result<Zeroizing<Key>, VaultError> {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            let bytes = super::store::device_key()?;
            let key: Key = bytes
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::NoDeviceKey)?;
            Ok(Zeroizing::new(key))
        }
        #[cfg(target_os = "android")]
        {
            android::open(self.app, &self.prompt)
        }
        #[cfg(windows)]
        {
            hello::open()
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android", windows)))]
        {
            Err(VaultError::NoDeviceKey)
        }
    }

    fn remove(&self) -> Result<(), VaultError> {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            super::store::set_device_key(None)
        }
        #[cfg(target_os = "android")]
        {
            android::forget(self.app)
        }
        #[cfg(windows)]
        {
            hello::forget()
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android", windows)))]
        {
            Ok(())
        }
    }

    fn changed(&self) -> bool {
        // The Keychain item has no notion of a new enrolment: it is behind
        // `userPresence`, which any enrolled face or the passcode answers.
        #[cfg(target_os = "android")]
        {
            android::changed(self.app)
        }
        #[cfg(windows)]
        {
            hello::changed()
        }
        #[cfg(not(any(target_os = "android", windows)))]
        {
            false
        }
    }
}

/// Whether this Mac will recognise somebody by their face or their finger.
#[cfg(target_os = "macos")]
fn sensor() -> bool {
    use objc2_local_authentication::{LAContext, LAPolicy};

    // The context is asked and dropped. Holding one would cache an answer, and
    // the answer changes: a fingerprint can be added or removed from System
    // Settings while the wallet is running.
    let context = unsafe { LAContext::new() };
    unsafe { context.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics) }
        .is_ok()
}

/// On iOS the item's own access policy is the whole of the check.
#[cfg(target_os = "ios")]
const fn sensor() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A keeper that does what it is told, and counts what it was asked.
    #[derive(Default)]
    struct Fake {
        unavailable: bool,
        proof: Option<Proof>,
        refuse: bool,
        /// Hands back something other than what it was given.
        garble: bool,
        changed: Cell<bool>,
        kept: RefCell<Option<Key>>,
        taken: Cell<u8>,
    }

    impl Keeper for Fake {
        fn available(&self) -> bool {
            !self.unavailable
        }
        fn put(&self, key: &Key) -> Result<Proof, VaultError> {
            if self.refuse {
                return Err(VaultError::DeviceRefused);
            }
            *self.kept.borrow_mut() = Some(*key);
            Ok(self.proof.unwrap_or(Proof::Given))
        }
        fn take(&self) -> Result<Zeroizing<Key>, VaultError> {
            self.taken.set(self.taken.get() + 1);
            if self.changed.get() {
                return Err(VaultError::DeviceChanged);
            }
            let mut key = self.kept.borrow().ok_or(VaultError::NoDeviceKey)?;
            if self.garble {
                key[0] ^= 1;
            }
            Ok(Zeroizing::new(key))
        }
        fn remove(&self) -> Result<(), VaultError> {
            *self.kept.borrow_mut() = None;
            Ok(())
        }
        fn changed(&self) -> bool {
            self.changed.get()
        }
    }

    fn opened() -> (Record, Key) {
        let (record, key) = Record::seal(&[7u8; 64], "246813", 6).unwrap();
        (record, *key)
    }

    #[test]
    fn a_copy_sealed_behind_the_check_is_armed_without_asking_twice() {
        let keeper = Fake::default();
        let (record, key) = opened();
        let armed = arm(&keeper, record, &key).unwrap();
        assert!(armed.device_wrap);
        assert_eq!(*keeper.kept.borrow(), Some(key));
        assert_eq!(keeper.taken.get(), 0);
    }

    #[test]
    fn a_copy_put_away_without_a_prompt_is_read_back_before_it_counts() {
        let keeper = Fake {
            proof: Some(Proof::Owed),
            ..Fake::default()
        };
        let (record, key) = opened();
        assert!(arm(&keeper, record.clone(), &key).unwrap().device_wrap);
        assert_eq!(keeper.taken.get(), 1);

        // Read back as something else: nobody was proven, nothing is left.
        let keeper = Fake {
            proof: Some(Proof::Owed),
            garble: true,
            ..Fake::default()
        };
        assert!(matches!(
            arm(&keeper, record, &key),
            Err(VaultError::DeviceUnproven)
        ));
        assert!(keeper.kept.borrow().is_none());
    }

    #[test]
    fn nobody_recognised_or_nowhere_to_keep_it_arms_nothing() {
        let (record, key) = opened();

        let refusing = Fake {
            refuse: true,
            ..Fake::default()
        };
        assert!(matches!(
            arm(&refusing, record.clone(), &key),
            Err(VaultError::DeviceUnproven)
        ));
        assert!(refusing.kept.borrow().is_none());

        let without = Fake {
            unavailable: true,
            ..Fake::default()
        };
        assert!(matches!(
            arm(&without, record, &key),
            Err(VaultError::NoDeviceStore)
        ));
        assert!(without.kept.borrow().is_none());
    }

    #[test]
    fn disarming_takes_the_copy_away() {
        let keeper = Fake::default();
        let (record, key) = opened();
        let armed = arm(&keeper, record, &key).unwrap();
        let disarmed = disarm(&keeper, armed).unwrap();
        assert!(!disarmed.device_wrap);
        assert!(keeper.kept.borrow().is_none());
    }

    #[test]
    fn an_armed_record_opens_with_the_copy_and_an_unarmed_one_does_not_ask() {
        let keeper = Fake::default();
        let (record, key) = opened();
        assert!(matches!(
            open(&keeper, &record),
            Err(VaultError::NoDeviceKey)
        ));
        assert_eq!(keeper.taken.get(), 0);

        let armed = arm(&keeper, record, &key).unwrap();
        match open(&keeper, &armed) {
            Ok(Opened::Key(taken)) => assert_eq!(*taken, key),
            _ => panic!("the copy should have opened it"),
        }
        assert_eq!(*armed.seed(&key).unwrap(), [7u8; 64]);
    }

    #[test]
    fn a_copy_that_can_no_longer_come_back_turns_biometrics_off_and_the_pin_stays() {
        let keeper = Fake::default();
        let (record, key) = opened();
        let armed = arm(&keeper, record, &key).unwrap();
        assert!(!changed(&keeper, &armed));

        // A fingerprint enrolled since.
        keeper.changed.set(true);
        assert!(changed(&keeper, &armed));
        match open(&keeper, &armed) {
            Ok(Opened::Changed(disarmed)) => {
                assert!(!disarmed.device_wrap);
                assert!(disarmed.has_pin());
                assert!(!changed(&keeper, &disarmed));
            }
            _ => panic!("a changed copy should disarm the record"),
        }
        assert!(keeper.kept.borrow().is_none());
    }

    #[test]
    fn a_record_the_device_alone_opens_is_never_disarmed() {
        let keeper = Fake::default();
        let (record, key) = Record::seal_device(&[7u8; 64]).unwrap();
        keeper.put(&key).unwrap();
        keeper.changed.set(true);
        assert!(!changed(&keeper, &record));
        assert!(matches!(
            open(&keeper, &record),
            Err(VaultError::DeviceChanged)
        ));
    }

    #[test]
    fn every_platform_but_linux_names_who_recognises_the_person() {
        let expected = if cfg!(any(target_os = "ios", target_os = "macos")) {
            Some("apple")
        } else if cfg!(target_os = "android") {
            Some("android")
        } else if cfg!(windows) {
            Some("windows")
        } else {
            None
        };
        assert_eq!(KIND.map(Kind::name), expected);
    }
}
