//! Password, passphrase and PIN generation (SPEC-V1 §7.3).
//!
//! Three things in this module are easy to get subtly wrong, and each is wrong
//! in a way that quietly weakens every password the user generates afterwards.
//!
//! **1. Modulo bias.** Reducing a random byte modulo a charset size that does
//! not divide 256 makes the first `256 % n` characters more likely than the
//! rest. [`uniform_index`] rejects out-of-range bytes instead.
//!
//! **2. Guaranteeing a class by substitution.** The obvious way to guarantee "at
//! least one digit" is to generate a string and then overwrite a position with a
//! digit. That conditions the distribution in a way the entropy formula does not
//! model, and it makes the substituted position predictable. SPEC-V1 §7.3 rules
//! it out: sample uniformly, and *reject and resample* strings that miss a
//! class.
//!
//! **3. Overstating entropy.** Rejection sampling removes strings from the
//! sample space, so `length × log2(charset)` is an upper bound, not the answer.
//! The true figure is `log2(|valid|)` by inclusion–exclusion, and §7.3 is blunt
//! about why it matters: *"Rev 1 demanded honesty and then gave the inflated
//! formula."* [`password_entropy_bits`] computes it in exact integers.
//!
//! A fourth is not a mistake so much as a temptation: a configurable separator
//! and optional capitalisation on a passphrase add **zero** bits, because the
//! attacker knows the scheme. Nothing here reports otherwise.

use zeroize::Zeroizing;

use crate::services::exact::Big;

/// Password length bounds (SPEC-V1 §7.3).
pub const MIN_PASSWORD_LEN: usize = 8;
/// Longest password the generator will produce.
pub const MAX_PASSWORD_LEN: usize = 128;
/// Default password length.
pub const DEFAULT_PASSWORD_LEN: usize = 20;

/// PIN length bounds (SPEC-V1 §7.3).
pub const MIN_PIN_LEN: usize = 3;
/// Longest PIN the generator will produce.
pub const MAX_PIN_LEN: usize = 12;
/// Default PIN length.
pub const DEFAULT_PIN_LEN: usize = 6;

/// Passphrase word-count bounds (SPEC-V1 §7.3).
pub const MIN_WORDS: usize = 3;
/// Most words a passphrase will use.
pub const MAX_WORDS: usize = 12;
/// Default word count.
pub const DEFAULT_WORDS: usize = 4;

/// Characters removed by "avoid ambiguous" (SPEC-V1 §7.3).
///
/// `|` is in the list the spec gives and not in our symbol set, so removing it
/// is a no-op. It stays here anyway: the set is the spec's, and silently
/// dropping a member because today's symbol set happens not to contain it is how
/// the two drift apart.
const AMBIGUOUS: &[char] = &['l', '1', 'I', '|', '0', 'O', 'o'];

const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";

/// The symbol set, exactly as SPEC-V1 §7.3 lists it.
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";

/// Rejection-sampling attempts before giving up.
///
/// Generous, and bounded. With the enforced minimum of 8 characters the
/// probability of missing a class is small enough that exhausting this is not a
/// distribution we could produce — but an unbounded loop over a CSPRNG is an
/// unbounded loop, and failing closed beats spinning.
const MAX_ATTEMPTS: usize = 10_000;

/// Why generation failed.
///
/// No variant carries a generated value or any part of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GeneratorError {
    /// The OS randomness source is unavailable. There is no fallback: a password
    /// from a degraded source is worse than no password (SPEC-V1 §3.2).
    #[error("the system randomness source is unavailable")]
    Rng,

    /// Rejection sampling did not find a valid string. Unreachable with the
    /// enforced bounds; reported rather than looped on forever.
    #[error("could not generate a password meeting the requested classes")]
    Exhausted,

    /// The word list is missing or the wrong size.
    #[error("the passphrase word list is unavailable")]
    NoWordList,
}

impl From<keyring_crypto::CryptoError> for GeneratorError {
    fn from(_: keyring_crypto::CryptoError) -> Self {
        Self::Rng
    }
}

/// Which character classes a password may draw from.
///
/// Four independent booleans, and they stay four booleans. SPEC-V1 §7.3 defines
/// exactly these toggles and the UI renders exactly these switches; folding them
/// into a bitmask to satisfy a lint would make `classes.digits` into
/// `classes.contains(Class::DIGITS)` and lose the one-to-one correspondence
/// between the type, the spec and the screen.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classes {
    /// `A`–`Z`.
    pub uppercase: bool,
    /// `a`–`z`.
    pub lowercase: bool,
    /// `0`–`9`.
    pub digits: bool,
    /// The §7.3 symbol set.
    pub symbols: bool,
}

impl Default for Classes {
    fn default() -> Self {
        Self {
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
        }
    }
}

impl Classes {
    /// The same selection with at least one class enabled.
    ///
    /// SPEC-V1 §7.3: *"At least one class always enabled (prevented, not
    /// error-handled)."* The UI prevents it; this makes the prevention true
    /// regardless of what reaches the function, and lowercase is the fallback
    /// because it is the largest class that needs no shift key.
    #[must_use]
    pub const fn normalised(self) -> Self {
        if self.uppercase || self.lowercase || self.digits || self.symbols {
            self
        } else {
            Self {
                uppercase: false,
                lowercase: true,
                digits: false,
                symbols: false,
            }
        }
    }

    /// How many classes are enabled.
    #[must_use]
    pub const fn count(self) -> usize {
        self.uppercase as usize
            + self.lowercase as usize
            + self.digits as usize
            + self.symbols as usize
    }
}

/// A password request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordOptions {
    /// Length, clamped to [`MIN_PASSWORD_LEN`]..=[`MAX_PASSWORD_LEN`].
    pub length: usize,
    /// Enabled character classes.
    pub classes: Classes,
    /// Remove [`AMBIGUOUS`] characters.
    pub avoid_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self {
            length: DEFAULT_PASSWORD_LEN,
            classes: Classes::default(),
            avoid_ambiguous: false,
        }
    }
}

impl PasswordOptions {
    /// Bounds applied, so the rest of the module can assume they hold.
    #[must_use]
    pub fn normalised(self) -> Self {
        Self {
            length: self.length.clamp(MIN_PASSWORD_LEN, MAX_PASSWORD_LEN),
            classes: self.classes.normalised(),
            avoid_ambiguous: self.avoid_ambiguous,
        }
    }
}

/// A generated secret and the honest entropy of the scheme that produced it.
///
/// The value is `Zeroizing`, so a caller that drops it without sending it
/// anywhere leaves nothing behind.
#[derive(Debug)]
pub struct Generated {
    /// The generated value.
    pub value: Zeroizing<String>,
    /// `floor(log2(|sample space|))`. Never the naive `length × log2(charset)`
    /// when rejection sampling narrowed the space.
    pub entropy_bits: u32,
}

/// The characters one class contributes, after ambiguity filtering.
fn class_chars(set: &str, avoid_ambiguous: bool) -> Vec<char> {
    set.chars()
        .filter(|c| !avoid_ambiguous || !AMBIGUOUS.contains(c))
        .collect()
}

/// Every enabled class, as separate character vectors.
///
/// Kept separate rather than concatenated because both the rejection test and
/// the inclusion–exclusion count need per-class membership.
fn enabled_classes(options: PasswordOptions) -> Vec<Vec<char>> {
    let mut out = Vec::with_capacity(4);
    for (enabled, set) in [
        (options.classes.uppercase, UPPERCASE),
        (options.classes.lowercase, LOWERCASE),
        (options.classes.digits, DIGITS),
        (options.classes.symbols, SYMBOLS),
    ] {
        if enabled {
            let chars = class_chars(set, options.avoid_ambiguous);
            if !chars.is_empty() {
                out.push(chars);
            }
        }
    }
    out
}

/// A uniformly distributed index in `0..n`, free of modulo bias.
///
/// Rejects bytes at or above the largest multiple of `n` that fits in a byte.
/// For `n = 95` that discards 66/256 of draws; the loop's expected iteration
/// count is under 1.4 and it cannot be biased by how many it takes.
fn uniform_index(n: usize) -> Result<usize, GeneratorError> {
    debug_assert!(n > 0 && n <= 256, "charset must fit a single byte draw");
    if n <= 1 {
        return Ok(0);
    }
    let limit = 256 - (256 % n);
    loop {
        let [byte] = keyring_crypto::rng::array::<1>()?;
        let value = usize::from(byte);
        if value < limit {
            return Ok(value % n);
        }
    }
}

/// Generate a password (SPEC-V1 §7.3).
///
/// Samples uniformly from the combined charset and resamples the whole string
/// until every enabled class appears — never substitutes a character to force a
/// class, which would bias the distribution the entropy figure describes.
///
/// # Errors
///
/// [`GeneratorError::Rng`] if the OS randomness source fails,
/// [`GeneratorError::Exhausted`] if rejection sampling does not converge, which
/// the enforced length bounds make unreachable.
pub fn password(options: PasswordOptions) -> Result<Generated, GeneratorError> {
    let options = options.normalised();
    let classes = enabled_classes(options);
    if classes.is_empty() {
        return Err(GeneratorError::Exhausted);
    }

    let alphabet: Vec<char> = classes.iter().flatten().copied().collect();
    let entropy_bits = password_entropy_bits(options);

    for _ in 0..MAX_ATTEMPTS {
        let mut candidate = Zeroizing::new(String::with_capacity(options.length));
        for _ in 0..options.length {
            let index = uniform_index(alphabet.len())?;
            candidate.push(*alphabet.get(index).ok_or(GeneratorError::Exhausted)?);
        }

        let complete = classes
            .iter()
            .all(|class| candidate.chars().any(|c| class.contains(&c)));
        if complete {
            return Ok(Generated {
                value: candidate,
                entropy_bits,
            });
        }
        // `candidate` zeroizes here. A rejected draw is still a random string of
        // the right shape, and leaving those in freed memory would be careless.
    }

    Err(GeneratorError::Exhausted)
}

/// Honest entropy for a password scheme, by inclusion–exclusion (SPEC-V1 §7.3).
///
/// ```text
/// |valid| = Σ_{S ⊆ classes} (−1)^|S| × (charset_size − Σ_{c∈S} |c|)^length
/// entropy = floor(log2(|valid|))
/// ```
///
/// Computed in exact integers, so it agrees with an independent implementation
/// rather than approximately agreeing (AC12). The sum is accumulated into
/// separate positive and negative halves and differenced once at the end, which
/// keeps every intermediate value non-negative.
///
/// Returns 0 when the space is empty, which cannot happen for a normalised
/// request but is the honest answer if it ever did.
#[must_use]
pub fn password_entropy_bits(options: PasswordOptions) -> u32 {
    let options = options.normalised();
    let classes = enabled_classes(options);
    if classes.is_empty() || options.length == 0 {
        return 0;
    }

    let total: usize = classes.iter().map(Vec::len).sum();
    let length = u32::try_from(options.length).unwrap_or(u32::MAX);

    let mut positive = Big::zero();
    let mut negative = Big::zero();

    // One bit per class: subset membership.
    for mask in 0..(1u32 << classes.len()) {
        let mut removed = 0usize;
        for (i, class) in classes.iter().enumerate() {
            if mask & (1 << i) != 0 {
                removed += class.len();
            }
        }
        let remaining = u32::try_from(total.saturating_sub(removed)).unwrap_or(0);
        let term = Big::pow(remaining, length);

        if mask.count_ones() % 2 == 0 {
            positive.add_assign(&term);
        } else {
            negative.add_assign(&term);
        }
    }

    // The identity guarantees this, and `sub_assign` asserts it.
    positive.sub_assign(&negative);
    positive.floor_log2().unwrap_or(0)
}

/// Generate a numeric PIN (SPEC-V1 §7.3).
///
/// Uniform over all digit strings of the requested length — no class
/// requirement, so no rejection and no correction to the entropy.
///
/// # Errors
///
/// [`GeneratorError::Rng`] if the OS randomness source fails.
pub fn pin(length: usize) -> Result<Generated, GeneratorError> {
    let length = length.clamp(MIN_PIN_LEN, MAX_PIN_LEN);
    let digits: Vec<char> = DIGITS.chars().collect();

    let mut value = Zeroizing::new(String::with_capacity(length));
    for _ in 0..length {
        let index = uniform_index(digits.len())?;
        value.push(*digits.get(index).ok_or(GeneratorError::Exhausted)?);
    }

    Ok(Generated {
        value,
        entropy_bits: pin_entropy_bits(length),
    })
}

/// `floor(length × log2(10))`.
#[must_use]
pub fn pin_entropy_bits(length: usize) -> u32 {
    let length = length.clamp(MIN_PIN_LEN, MAX_PIN_LEN);
    Big::pow(10, u32::try_from(length).unwrap_or(0))
        .floor_log2()
        .unwrap_or(0)
}

// ── Passphrase (SPEC-V1 §7.3) ───────────────────────────────────────────────

/// How a passphrase is assembled.
///
/// `separator` and `capitalise` are presentation. They add **zero** bits — the
/// attacker knows the scheme — and [`passphrase_entropy_bits`] ignores them
/// deliberately. Never let a UI imply otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseOptions {
    /// Word count, clamped to [`MIN_WORDS`]..=[`MAX_WORDS`].
    pub words: usize,
    /// String placed between words.
    pub separator: String,
    /// Capitalise each word's first letter.
    pub capitalise: bool,
    /// Append a digit. Adds `log2(10)` bits, floored into the total.
    pub numeric_suffix: bool,
}

impl Default for PassphraseOptions {
    fn default() -> Self {
        Self {
            words: DEFAULT_WORDS,
            separator: "-".to_owned(),
            capitalise: false,
            numeric_suffix: false,
        }
    }
}

/// Words the EFF long list is required to contain (SPEC-V1 §7.3).
pub const EFF_WORDLIST_LEN: usize = 7_776;

/// The vendored EFF long wordlist, when the asset is present.
///
/// `build.rs` decides presence and sets `has_wordlist`, because `include_str!` on
/// a missing file is a compile error and the asset's licence is still unconfirmed
/// (THIRD-PARTY-NOTICES.md). Dropping the file in and rebuilding turns the feature
/// on; until then [`passphrase`] reports it unavailable rather than generating
/// from whatever happens to be there.
#[cfg(has_wordlist)]
const WORDLIST_RAW: &str = include_str!("../../assets/eff_large_wordlist.txt");

/// The bundled wordlist, or `None` when the asset is not vendored.
///
/// Each line is `<5 dice digits>\t<word>`; only the word is taken. The length is
/// **not** checked here — [`passphrase`] does that, so the same refusal covers a
/// bundled list that is the wrong size and a caller-supplied one that is.
#[must_use]
pub fn bundled_wordlist() -> Option<Vec<&'static str>> {
    #[cfg(has_wordlist)]
    {
        Some(
            WORDLIST_RAW
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|line| line.rsplit('\t').next().unwrap_or(line).trim())
                .collect(),
        )
    }
    #[cfg(not(has_wordlist))]
    {
        None
    }
}

/// Build a passphrase from a supplied word list.
///
/// The list is a parameter rather than a compiled-in constant so that the
/// *algorithm* is testable and reviewable before the EFF list is vendored, and
/// so the vendored list is a data file with a licence recorded next to it rather
/// than 7,776 string literals in a source tree.
///
/// # Errors
///
/// [`GeneratorError::NoWordList`] if `words` is not exactly
/// [`EFF_WORDLIST_LEN`] entries — a short list silently costs entropy, and
/// "silently costs entropy" is the failure this whole module is written against.
/// [`GeneratorError::Rng`] if the OS randomness source fails.
pub fn passphrase(
    options: &PassphraseOptions,
    words: &[&str],
) -> Result<Generated, GeneratorError> {
    if words.len() != EFF_WORDLIST_LEN {
        return Err(GeneratorError::NoWordList);
    }
    let count = options.words.clamp(MIN_WORDS, MAX_WORDS);

    let mut parts: Vec<String> = Vec::with_capacity(count);
    for _ in 0..count {
        let index = uniform_index_wide(words.len())?;
        let word = words.get(index).ok_or(GeneratorError::NoWordList)?;
        parts.push(if options.capitalise {
            capitalise_first(word)
        } else {
            (*word).to_owned()
        });
    }

    let mut value = Zeroizing::new(parts.join(&options.separator));
    if options.numeric_suffix {
        let digit = uniform_index(10)?;
        value.push_str(&digit.to_string());
    }

    Ok(Generated {
        value,
        entropy_bits: passphrase_entropy_bits(options),
    })
}

/// `floor(words × log2(7776) + suffix_bits)` (SPEC-V1 §7.3).
///
/// Computed as `floor(log2(7776^words × 10^suffix))` in exact integers rather
/// than by summing floating-point logs, so it cannot drift by a bit at a
/// boundary. Separator and capitalisation contribute nothing.
#[must_use]
pub fn passphrase_entropy_bits(options: &PassphraseOptions) -> u32 {
    let count = u32::try_from(options.words.clamp(MIN_WORDS, MAX_WORDS)).unwrap_or(0);
    let mut space = Big::pow(u32::try_from(EFF_WORDLIST_LEN).unwrap_or(0), count);
    if options.numeric_suffix {
        space.mul_u32(10);
    }
    space.floor_log2().unwrap_or(0)
}

/// A uniform index into a list larger than 256 entries.
///
/// Same rejection strategy as [`uniform_index`], widened to a 32-bit draw so a
/// 7,776-word list is addressable without bias.
fn uniform_index_wide(n: usize) -> Result<usize, GeneratorError> {
    if n <= 1 {
        return Ok(0);
    }
    let n_u64 = n as u64;
    let limit = (u64::from(u32::MAX) + 1) - ((u64::from(u32::MAX) + 1) % n_u64);
    loop {
        let bytes = keyring_crypto::rng::array::<4>()?;
        let value = u64::from(u32::from_le_bytes(bytes));
        if value < limit {
            return Ok(usize::try_from(value % n_u64).unwrap_or(0));
        }
    }
}

fn capitalise_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
