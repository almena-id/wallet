//! The words an identity is made of: making them, and reading them back.

use bip39::{Language, Mnemonic};
use zeroize::Zeroizing;

use super::IdentityError;

/// How long a phrase is, in words.
///
/// **Twelve or twenty-four, and nothing in between.** BIP-39 also defines
/// fifteen, eighteen and twenty-one, and they are left out on purpose: the two
/// offered here are the two every other wallet offers, so a phrase made in this
/// one is a phrase somebody can bring somewhere else. Twelve is the default
/// because it is what most wallets produce; twenty-four is there because the
/// people who want 256 bits behind their identity want it for reasons this
/// wallet is in no position to argue with.
///
/// Both are read back on restore regardless of which one made the phrase, and a
/// phrase of either length derives its seed the same way — the length is a
/// property of the words, not of the identity they produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    Twelve,
    TwentyFour,
}

impl Length {
    /// Both lengths, in the order they are offered.
    pub const ALL: [Self; 2] = [Self::Twelve, Self::TwentyFour];

    /// The length a word count names.
    ///
    /// # Errors
    ///
    /// [`IdentityError::WordCount`] for any other number. The interface offers
    /// two buttons, so a third number is a caller that has gone wrong rather
    /// than a person who has.
    pub fn of(words: usize) -> Result<Self, IdentityError> {
        Self::ALL
            .into_iter()
            .find(|length| length.words() == words)
            .ok_or(IdentityError::WordCount)
    }

    /// How many words it is.
    pub const fn words(self) -> usize {
        match self {
            Self::Twelve => 12,
            Self::TwentyFour => 24,
        }
    }

    /// The entropy behind it, in bytes.
    ///
    /// BIP-39 fixes the relation: a word carries 11 bits, and a phrase spends
    /// one checksum bit for every 32 bits of entropy. Sixteen bytes come back as
    /// twelve words, thirty-two as twenty-four.
    const fn entropy_bytes(self) -> usize {
        match self {
            Self::Twelve => 16,
            Self::TwentyFour => 32,
        }
    }
}

/// The most entropy any length asks for, which is what the buffer is sized to.
const MAX_ENTROPY_BYTES: usize = 32;

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
pub fn generate(language: Language, length: Length) -> Result<Mnemonic, IdentityError> {
    // Wiped when this returns: it is the whole of what an identity is made from,
    // and a copy left on the stack is a copy of somebody's phrase. The buffer is
    // sized for the longest phrase and only the part this length asks for is
    // filled, so a shorter phrase still leaves nothing behind it.
    let mut buffer = Zeroizing::new([0u8; MAX_ENTROPY_BYTES]);
    let entropy = &mut buffer[..length.entropy_bytes()];
    getrandom::getrandom(entropy).map_err(|_| IdentityError::Entropy)?;

    Mnemonic::from_entropy_in(language, entropy).map_err(|_| IdentityError::Entropy)
}

/// Reads a phrase somebody typed or pasted, in whichever language they wrote it.
///
/// Every wordlist this build carries is tried, so a phrase made in another
/// wallet, in any language BIP-39 defines, is one this one can bring back.
///
/// **The length is not asked for, it is counted.** Somebody bringing an identity
/// back writes down what they have; being made to declare twelve or twenty-four
/// first would only add a question they can get wrong about words that already
/// say which they are.
///
/// Normalised first — lower case, and any run of whitespace becomes one space —
/// so a phrase pasted out of a note with line breaks in it is the same phrase.
/// Then counted, then checked: each step answers a different question about what
/// is wrong with it, and the frontend says so in the person's own language.
///
/// # Errors
///
/// [`IdentityError::WordCount`] when the words are not one of the lengths in
/// [`Length`], and [`IdentityError::Checksum`] when they are not a phrase this
/// scheme ever produced — a typo, or words in the wrong order.
pub fn read(input: &str) -> Result<Mnemonic, IdentityError> {
    let joined = Zeroizing::new(
        input
            .split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
            .join(" "),
    );

    let count = joined.split(' ').filter(|word| !word.is_empty()).count();
    Length::of(count)?;

    // Every wordlist this build carries is tried, so somebody who wrote their
    // phrase in Spanish is not asked to remember that they did.
    Mnemonic::parse_normalized(&joined).map_err(|_| IdentityError::Checksum)
}
