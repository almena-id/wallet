//! Making an identity: the words it is made of, the key they produce, and the
//! DID document that describes it.
//!
//! **The phrase is never written down.** It is held only between being shown and
//! being confirmed, and then it is gone: it is the backup, and the backup
//! belongs to the person rather than to the device.
//!
//! The seed it produces is held here for as long as the wallet is open, because
//! signing a request needs the key and asking for the phrase every time is not
//! a wallet. Between launches it is [`crate::vault`] that keeps it, encrypted
//! and behind a PIN — this module derives, and knows nothing about where.

pub(crate) mod keys;
mod phrase;

use std::sync::Mutex;

use bip39::Mnemonic;
use serde::{Serialize, Serializer};
use tauri::{Manager, State};
use zeroize::Zeroizing;

/// What can go wrong, as codes rather than prose.
///
/// The frontend holds the catalogues, so this side names the failure and the
/// interface says it in the language somebody is reading.
#[derive(Debug, Clone, Copy)]
pub enum IdentityError {
    /// The operating system would not supply randomness.
    Entropy,
    /// The phrase is not one of the lengths [`phrase::Length`] allows.
    WordCount,
    /// The right number of words, but not a phrase this scheme ever produced.
    Checksum,
    /// Asked to finish a creation that was never started.
    NoDraft,
}

impl IdentityError {
    const fn code(self) -> &'static str {
        match self {
            Self::Entropy => "identity_entropy_unavailable",
            Self::WordCount => "identity_word_count",
            Self::Checksum => "identity_checksum",
            Self::NoDraft => "identity_no_draft",
        }
    }
}

impl Serialize for IdentityError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// The phrase somebody is being shown, held between the two halves of creating
/// an identity: the words on screen, and the confirmation that they wrote them
/// down. It never reaches disk and is dropped the moment it is used or replaced.
#[derive(Default)]
pub struct Draft(Mutex<Option<Mnemonic>>);

/// The seed of the identity that is open, for as long as it is open.
///
/// Every key this wallet ever signs with comes from here, so it never crosses
/// to the interface and it never reaches disk. Signing out drops it, and so
/// does closing the wallet.
#[derive(Default)]
pub struct Held(Mutex<Option<Zeroizing<[u8; 64]>>>);

impl Held {
    /// The seed itself, for the one caller that has business with it.
    ///
    /// [`crate::vault`] is what writes an identity down, and writing it down
    /// means holding it for as long as it takes to seal it. It comes back
    /// wrapped so the copy is wiped rather than left on a stack, and it goes
    /// nowhere else: nothing in the interface can ask for this.
    pub(crate) fn seed(&self) -> Option<Zeroizing<[u8; 64]>> {
        self.0
            .lock()
            .expect("the seed lock is never held across a panic")
            .clone()
    }

    /// Lets go of whatever is open.
    pub(crate) fn forget(&self) {
        *self
            .0
            .lock()
            .expect("the seed lock is never held across a panic") = None;
    }
}

/// An identity, as the interface needs it.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    /// The identifier itself, `did:key:z…`.
    pub did: String,
    /// The public key as multibase text, which is what the identifier is built from.
    pub public_key: String,
    /// The DID document, ready to be handed to the platform when there is one to hand it to.
    pub document: serde_json::Value,
}

/// A new phrase, held for confirmation and handed to the interface to show.
///
/// `locale` is the language the interface is in: the wordlist follows it, so the
/// words are ones the person reads rather than ones they transcribe. `words` is
/// the length somebody chose — see [`phrase::Length`] for why there are two.
///
/// Asking again replaces whatever was being held, which is what switching
/// length is: the phrase that was on the screen is dropped unshown, and the one
/// that comes back has to be written down from scratch.
///
/// # Errors
///
/// [`IdentityError::WordCount`] when `words` is not a length this wallet makes,
/// and [`IdentityError::Entropy`] when the system will not supply randomness.
#[tauri::command]
pub fn identity_draft(
    draft: State<'_, Draft>,
    locale: String,
    words: usize,
) -> Result<Vec<String>, IdentityError> {
    let length = phrase::Length::of(words)?;
    let mnemonic = phrase::generate(phrase::language_for(&locale), length)?;
    let shown = mnemonic.words().map(str::to_string).collect();
    *draft
        .0
        .lock()
        .expect("the draft lock is never held across a panic") = Some(mnemonic);
    Ok(shown)
}

/// The identity the held phrase produces, and the end of that phrase's stay here.
///
/// # Errors
///
/// [`IdentityError::NoDraft`] when no phrase is being shown — the interface asked
/// to finish something it never started.
#[tauri::command]
pub fn identity_create(
    draft: State<'_, Draft>,
    held: State<'_, Held>,
) -> Result<Identity, IdentityError> {
    let mnemonic = draft
        .0
        .lock()
        .expect("the draft lock is never held across a panic")
        .take()
        .ok_or(IdentityError::NoDraft)?;

    Ok(from_phrase(&mnemonic, &held))
}

/// The identity a phrase somebody already has produces.
///
/// The same words always give the same identity, which is the whole of what
/// signing in means here: there is no account to look up, only a phrase to
/// derive from.
///
/// # Errors
///
/// [`IdentityError::WordCount`] or [`IdentityError::Checksum`], depending on what
/// is wrong with what they wrote.
#[tauri::command]
pub fn identity_restore(input: String, held: State<'_, Held>) -> Result<Identity, IdentityError> {
    restore(&input, &held)
}

/// What `identity_restore` does, without the command wrapper around it, so the
/// tests can reach it without a running application.
fn restore(input: &str, held: &Held) -> Result<Identity, IdentityError> {
    let mnemonic = phrase::read(input)?;
    Ok(from_phrase(&mnemonic, held))
}

/// Forgets the phrase being shown, for somebody who left the flow.
#[tauri::command]
pub fn identity_discard(draft: State<'_, Draft>) {
    *draft
        .0
        .lock()
        .expect("the draft lock is never held across a panic") = None;
}

/// Lets go of the identity that was open.
///
/// Signing out is this and nothing else: there is nowhere it was written, so
/// there is nothing to erase — only a seed to drop.
#[tauri::command]
pub fn identity_forget(draft: State<'_, Draft>, held: State<'_, Held>) {
    *draft
        .0
        .lock()
        .expect("the draft lock is never held across a panic") = None;
    held.forget();
}

/// Registers the state the creation flow and the open identity need.
pub fn manage<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    app.manage(Draft::default());
    app.manage(Held::default());
}

/// The identity a phrase produces: BIP-39 seed, one hardened SLIP-0010 step, and
/// the `did:key` that names the public key it ends at.
fn from_phrase(mnemonic: &Mnemonic, held: &Held) -> Identity {
    // No passphrase. A second secret nobody was told to write down is a second
    // way to lose an identity, and the words are already the whole of it.
    adopt(Zeroizing::new(mnemonic.to_seed_normalized("")), held)
}

/// The identity a seed names, held open from here on.
///
/// The other way in: [`crate::vault`] unseals a seed that was written down on a
/// previous launch, and this is what turns it back into an open wallet. The
/// words are not involved and are not needed — they produced this seed once and
/// their job was done then.
pub(crate) fn adopt(seed: Zeroizing<[u8; 64]>, held: &Held) -> Identity {
    let signing_key = keys::signing_key(&seed);
    let public_key = keys::written(&signing_key.verifying_key().to_bytes());
    let did = format!("did:key:{public_key}");

    // Kept for as long as the wallet is open: every request answered from now
    // on is signed with a key derived from here.
    *held
        .0
        .lock()
        .expect("the seed lock is never held across a panic") = Some(seed);

    Identity {
        document: document(&did, &public_key),
        did,
        public_key,
    }
}

/// The DID document for a `did:key` identity.
///
/// It is not stored anywhere and it does not need to be: `did:key` documents are
/// derived from the identifier itself, so anybody holding the DID can rebuild
/// this exactly. It is built here because the platform's API will want it, and
/// because somebody creating an identity should be able to see what it says.
fn document(did: &str, public_key: &str) -> serde_json::Value {
    let method = format!("{did}#{public_key}");

    serde_json::json!({
        "@context": [
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/multikey/v1"
        ],
        "id": did,
        "verificationMethod": [{
            "id": method,
            "type": "Multikey",
            "controller": did,
            "publicKeyMultibase": public_key
        }],
        "authentication": [method],
        "assertionMethod": [method],
        "capabilityInvocation": [method],
        "capabilityDelegation": [method]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The vector that pins the derivation.** A phrase, and the identity it has
    /// to keep producing. If this changes, everybody's words start opening
    /// a different identity — see the note at the top of `keys.rs`.
    const PHRASE: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    /// The identifier those words have to keep producing, worked out from the
    /// three published standards this chain is made of rather than from a run
    /// of this code: BIP-39's own vector for the phrase above, SLIP-0010 for the
    /// step down to `m/0'` — see `keys.rs`, where both its vectors are run — and
    /// the multicodec `did:key` puts in front of an ed25519 key.
    ///
    /// Which is what makes this a check and not a photograph. A pin taken from
    /// whatever the code happened to answer would keep agreeing with the code
    /// after both had drifted away from the standard together.
    const IDENTITY: &str = "did:key:z6MkrTgzDs6XmRgSKZZhMLvmPm1obfjazbpZ8so3FzchHJhL";

    #[test]
    fn the_same_words_always_give_the_same_identity() {
        let held = Held::default();
        let first = restore(PHRASE, &held).expect("a valid phrase");
        let second = restore(PHRASE, &held).expect("a valid phrase");
        assert_eq!(first.did, second.did);
        assert_eq!(first.did, IDENTITY);
    }

    #[test]
    fn the_document_describes_the_identifier() {
        let identity = restore(PHRASE, &Held::default()).expect("a valid phrase");
        assert_eq!(identity.document["id"], identity.did);
        assert_eq!(
            identity.document["verificationMethod"][0]["publicKeyMultibase"],
            identity.public_key
        );
    }

    #[test]
    fn the_wordlist_follows_the_language_the_wallet_is_read_in() {
        use bip39::Language;

        for (locale, expected) in [
            ("es", Language::Spanish),
            ("es-ES", Language::Spanish),
            ("fr", Language::French),
            ("pt-BR", Language::Portuguese),
            ("ja", Language::Japanese),
            ("zh-Hant", Language::TraditionalChinese),
            // A bare `zh` is not a wordlist, so it takes the one most writing in
            // Chinese uses rather than refusing to answer.
            ("zh", Language::SimplifiedChinese),
            // German has no BIP-39 wordlist, and neither has nothing at all.
            ("de", Language::English),
            ("", Language::English),
        ] {
            assert_eq!(phrase::language_for(locale), expected, "locale {locale}");
        }
    }

    #[test]
    fn a_phrase_written_in_another_language_still_comes_back() {
        // A phrase made in a language this wallet does not show its interface
        // in — as another wallet would have made it — is one this wallet reads.
        for language in [bip39::Language::French, bip39::Language::Japanese] {
            let written = phrase::generate(language, phrase::Length::Twelve)
                .expect("entropy")
                .words()
                .collect::<Vec<_>>()
                .join(" ");
            let identity = restore(&written, &Held::default()).expect("a valid phrase");
            assert!(identity.did.starts_with("did:key:z6Mk"), "{language:?}");
        }
    }

    /// Both lengths are made and both come back. The point is not that the two
    /// identities differ — any two phrases differ — but that neither length is
    /// refused on the way in, which is the whole of what choosing one means.
    #[test]
    fn either_length_makes_a_phrase_that_comes_back() {
        for length in phrase::Length::ALL {
            let mnemonic = phrase::generate(bip39::Language::English, length).expect("entropy");
            let written = mnemonic.words().collect::<Vec<_>>();
            assert_eq!(written.len(), length.words(), "{length:?}");

            let identity = restore(&written.join(" "), &Held::default()).expect("a valid phrase");
            assert!(identity.did.starts_with("did:key:z6Mk"), "{length:?}");
        }
    }

    /// Twenty-four words, from BIP-39's own vector for thirty-two bytes of
    /// zero entropy, and the identity they have to keep producing — worked out
    /// the same way [`IDENTITY`] was, from the standards rather than from a run
    /// of this code. A longer phrase is not a different derivation: it reaches
    /// the same 64-byte seed the same way, and everything below the seed is the
    /// chain `keys.rs` already pins.
    #[test]
    fn a_twenty_four_word_phrase_derives_the_identity_the_standards_name() {
        const LONG_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
        const LONG_IDENTITY: &str = "did:key:z6MkpBPdwmZU5K3HxiZc4oEzo7UPTBVwZGa48Ce1DvMn9C8V";

        let identity = restore(LONG_PHRASE, &Held::default()).expect("a valid phrase");
        assert_eq!(identity.did, LONG_IDENTITY);
    }

    #[test]
    fn a_phrase_of_the_wrong_length_is_refused_before_its_checksum() {
        // Two words, and eighteen: BIP-39 defines eighteen and this wallet does
        // not offer it, so it is refused here like any other number that is not
        // one of the two.
        for count in [2, 18] {
            let words = vec!["abandon"; count].join(" ");
            assert!(
                matches!(
                    restore(&words, &Held::default()),
                    Err(IdentityError::WordCount)
                ),
                "{count} words"
            );
        }
    }

    #[test]
    fn words_of_a_right_length_that_are_not_a_phrase_are_refused() {
        for length in phrase::Length::ALL {
            let words = vec!["abandon"; length.words()].join(" ");
            assert!(
                matches!(
                    restore(&words, &Held::default()),
                    Err(IdentityError::Checksum)
                ),
                "{length:?}"
            );
        }
    }
}
