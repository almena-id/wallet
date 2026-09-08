//! Whether this machine can recognise the person in front of it.
//!
//! **Not the same question as whether there is somewhere to keep a key.** A Mac
//! mini with an ordinary keyboard has the data protection keychain and no
//! sensor at all, and `kSecAccessControlUserPresence` on such a machine falls
//! back to the login password. That is still a lock the system enforces — but a
//! switch labelled with a fingerprint that asks for a password is a switch that
//! lied, so it is not offered where there is no finger to read.
//!
//! iOS is not asked. Every device the wallet runs on there has Face ID, Touch ID
//! or a passcode, one of the three is always set up by the time an identity
//! exists, and the item's own access control is what enforces it.

/// Whether the system will recognise somebody by their face or their finger.
#[cfg(target_os = "macos")]
pub fn available() -> bool {
    use objc2_local_authentication::{LAContext, LAPolicy};

    // The context is asked and dropped. Holding one would cache an answer, and
    // the answer changes: a fingerprint can be added or removed from System
    // Settings while the wallet is running.
    let context = unsafe { LAContext::new() };
    unsafe { context.canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics) }
        .is_ok()
}

/// Everywhere else the store's own access policy is the whole of the check.
#[cfg(not(target_os = "macos"))]
pub const fn available() -> bool {
    true
}
