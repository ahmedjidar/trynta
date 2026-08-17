//! Password strength (SPEC-V1 §7.4).
//!
//! > **Weak** — `zxcvbn`. Weak = offline crack time under 1 day at 10⁴ guesses/s.
//! > Show the estimate.
//!
//! Three things about that sentence decide the implementation.
//!
//! **"Offline crack time"** means the threshold is a guess count, not a score.
//! `zxcvbn` reports a 0–4 score, and it is tempting to call 0–2 weak — but the
//! score is a coarse bucketing of the same guess estimate, and the spec gives a
//! number. `10⁴ guesses/s × 86,400 s = 8.64 × 10⁸ guesses` is the line, so that is
//! what [`assess`] compares against.
//!
//! **"Show the estimate"** means the caller needs the number, not just the verdict.
//! A user told "weak" learns nothing actionable; a user told "about four hours"
//! can decide. So [`Strength`] carries the guess count and a derived crack time
//! rather than only a band.
//!
//! **10⁴ guesses/s is a deliberately conservative attacker.** It is far below what
//! a GPU does against a fast hash, and that is the right direction to be wrong in:
//! it flags more passwords as weak, not fewer. The figure is the spec's and is not
//! ours to relax.
//!
//! Offline by construction. `zxcvbn`'s dictionaries are compiled in, so this makes
//! no request — which matters, because a strength estimator that consulted a
//! service would be a fourth outbound request and §7 permits three.
//!
//! ## Item fields are fed in as user inputs
//!
//! `zxcvbn` scores a password lower when it contains context the attacker would
//! guess first. A password of `acme-alice-2026` on an item titled "Acme" for
//! `alice@acme.test` is far weaker than its character count suggests, and passing
//! the title, username and URLs as user inputs is what lets the estimator see
//! that. Those are non-secret metadata the report already holds.

use zeroize::Zeroizing;

/// Guesses per second the threshold assumes (SPEC-V1 §7.4).
pub const GUESSES_PER_SECOND: u64 = 10_000;

/// Seconds in the "under 1 day" threshold.
pub const THRESHOLD_SECONDS: u64 = 86_400;

/// Guess count at or below which a password is weak.
///
/// `10⁴ guesses/s` for one day. Written as the product so the derivation is
/// visible rather than a magic constant.
pub const WEAK_AT_OR_BELOW_GUESSES: u64 = GUESSES_PER_SECOND * THRESHOLD_SECONDS;

/// The four-band scale the UI renders (SPEC-V1 §7.2, the strength meter).
///
/// Distinct from the weak/not-weak verdict: the meter has four segments and the
/// report has a threshold, and conflating them is how a meter ends up disagreeing
/// with the risk list beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    /// Guessable almost immediately.
    VeryWeak,
    /// Under the §7.4 threshold.
    Weak,
    /// Above the threshold but not comfortably.
    Fair,
    /// Strong.
    Strong,
}

/// What is known about one password's strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strength {
    /// Estimated guesses to crack.
    pub guesses: u64,
    /// Seconds at [`GUESSES_PER_SECOND`]. Saturates rather than overflowing on a
    /// very strong password, where the honest answer is "longer than you care
    /// about" and the exact figure is meaningless anyway.
    pub crack_seconds: u64,
    /// Whether this is weak by §7.4's definition.
    pub weak: bool,
    /// The band the meter renders.
    pub band: Band,
}

/// Assess a password, with the item's own non-secret fields as context.
///
/// `context` should be the item's title, username and URLs. Passing them lowers
/// the estimate for a password built out of them, which is the estimate an
/// attacker who can see the item would work from.
///
/// The password is taken as `&str` rather than by value so no copy is made here;
/// callers hold it in a `Zeroizing` buffer.
#[must_use]
pub fn assess(password: &str, context: &[&str]) -> Strength {
    if password.is_empty() {
        return Strength {
            guesses: 0,
            crack_seconds: 0,
            weak: true,
            band: Band::VeryWeak,
        };
    }

    let entropy = zxcvbn::zxcvbn(password, context);
    let guesses = entropy.guesses();
    let crack_seconds = guesses / GUESSES_PER_SECOND;
    let weak = guesses <= WEAK_AT_OR_BELOW_GUESSES;

    Strength {
        guesses,
        crack_seconds,
        weak,
        band: band_for(guesses),
    }
}

/// Map a guess count onto the meter's four segments.
///
/// The boundaries are anchored to the §7.4 threshold so the meter and the risk
/// list cannot disagree: everything the report calls weak lands in
/// [`Band::VeryWeak`] or [`Band::Weak`], and nothing else does.
fn band_for(guesses: u64) -> Band {
    // An hour at the assumed rate. Below this, "weak" understates it.
    const VERY_WEAK_CEILING: u64 = GUESSES_PER_SECOND * 3_600;
    // A hundred days. Above the threshold but not a password to leave in place.
    const FAIR_CEILING: u64 = GUESSES_PER_SECOND * THRESHOLD_SECONDS * 100;

    if guesses <= VERY_WEAK_CEILING {
        Band::VeryWeak
    } else if guesses <= WEAK_AT_OR_BELOW_GUESSES {
        Band::Weak
    } else if guesses <= FAIR_CEILING {
        Band::Fair
    } else {
        Band::Strong
    }
}

/// Assess a password held in a zeroizing buffer.
///
/// A convenience so callers on the secret path do not have to reach through the
/// wrapper and risk binding a plain `&str` that outlives it.
#[must_use]
pub fn assess_secret(password: &Zeroizing<String>, context: &[&str]) -> Strength {
    assess(password, context)
}
