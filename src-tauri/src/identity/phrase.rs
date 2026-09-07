//! The twelve words an identity is made of: making them, and reading them back.

use bip39::{Language, Mnemonic};
use zeroize::Zeroizing;

use super::IdentityError;

/// How many words a phrase has. Twelve, everywhere, always.
pub const WORDS: usize = 12;

/// The entropy behind twelve words, in bytes. BIP-39 fixes the relation.
const ENTROPY_BYTES: usize = 16;

/// The BCP 47 tag of a wordlist, which is how a locale finds it.
///
/// Every language BIP-39 defines is named here, because `Cargo.toml` turns on
/// every wordlist the crate has. Trimming that feature list takes variants out
/// of `Language`, and this match stops compiling until they are taken out here
/// too — which is the loud failure and not the quiet one.
fn tag(language: Language) -> &'static str {
    match language {
        Language::English => "en",
        Language::SimplifiedChinese => "zh-Hans",
        Language::TraditionalChinese => "zh-Hant",
        Language::Czech => "cs",
        Language::French => "fr",
        Language::Italian => "it",
        Language::Japanese => "ja",
        Language::Korean => "ko",
        Language::Portuguese => "pt",
        Language::Spanish => "es",
    }
}

/// The wordlist a locale writes its phrase in.
///
/// A phrase is the identity, so it is written in the language the person is
/// reading the wallet in — BIP-39 has wordlists for exactly that. A language the
/// standard has no words for falls back to English, which is the list every
/// implementation carries and the one every other wallet can read.
///
/// The whole tag is tried before its language subtag, because `zh-Hans` and
/// `zh-Hant` are two wordlists and not one: a phrase written in the wrong one
/// opens a different identity.
pub fn language_for(locale: &str) -> Language {
    let wanted = locale.trim().to_lowercase();
    if wanted.is_empty() {
        return Language::English;
    }

    if let Some(exact) = Language::ALL
        .iter()
        .find(|language| tag(**language).to_lowercase() == wanted)
    {
        return *exact;
    }

    let primary = wanted.split(['-', '_']).next().unwrap_or_default();
    Language::ALL
        .iter()
        .copied()
        .find(|language| tag(*language).split('-').next() == Some(primary))
        .unwrap_or(Language::English)
}

/// A new phrase, from the operating system's entropy and from nothing else.
///
/// # Errors
///
/// [`IdentityError::Entropy`] when the system will not supply randomness. There
/// is no fallback and there must not be one: a phrase made from a predictable
/// number is worse than no phrase at all.
pub fn generate(language: Language) -> Result<Mnemonic, IdentityError> {
    // Wiped when this returns: it is the whole of what an identity is made from,
    // and a copy left on the stack is a copy of somebody's twelve words.
    let mut entropy = Zeroizing::new([0u8; ENTROPY_BYTES]);
    getrandom::getrandom(entropy.as_mut_slice()).map_err(|_| IdentityError::Entropy)?;

    Mnemonic::from_entropy_in(language, entropy.as_slice()).map_err(|_| IdentityError::Entropy)
}

/// Reads a phrase somebody typed or pasted, in whichever language they wrote it.
///
/// Every wordlist this build carries is tried, so a phrase made in another
/// wallet, in any language BIP-39 defines, is one this one can bring back.
///
/// Normalised first — lower case, and any run of whitespace becomes one space —
/// so a phrase pasted out of a note with line breaks in it is the same phrase.
/// Then counted, then checked: each step answers a different question about what
/// is wrong with it, and the frontend says so in the person's own language.
///
/// # Errors
///
/// [`IdentityError::WordCount`] when there are not exactly [`WORDS`] words, and
/// [`IdentityError::Checksum`] when the words are not a phrase this scheme ever
/// produced — a typo, or words in the wrong order.
pub fn read(input: &str) -> Result<Mnemonic, IdentityError> {
    let joined = Zeroizing::new(
        input
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" "),
    );

    if joined.split(' ').filter(|word| !word.is_empty()).count() != WORDS {
        return Err(IdentityError::WordCount);
    }

    // Every wordlist this build carries is tried, so somebody who wrote their
    // phrase in Spanish is not asked to remember that they did.
    Mnemonic::parse_normalized(&joined).map_err(|_| IdentityError::Checksum)
}
