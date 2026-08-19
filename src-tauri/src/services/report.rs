//! The vault health score and reuse detection (SPEC-V1 §7.4).
//!
//! The score is required to be **deterministic, explainable, and shown with its
//! breakdown**, which is why this module computes it from a plain struct of
//! counts rather than reaching for the vault. Every number that goes into it can
//! be put on screen next to the result, and the same inputs always give the same
//! answer.
//!
//! ```text
//! N = live login items
//! if N == 0:  score = null → "not enough data"   (not 0, not 100)
//! reused = items participating in ANY reuse group   (3 sharing one password → 3, not 1)
//!
//! base = 35×(1 − breached/N) + 25×(1 − weak/N) + 20×(1 − reused/N) + 20×(2fa_enabled/2fa_capable)
//! if 2fa_capable == 0:  weights become 43.75 / 31.25 / 25
//! round ONCE, at the end, after redistribution.  Clamp 0–100.
//! ```
//!
//! Three details the spec spells out because they are the ones people get wrong:
//!
//! - **`N == 0` is `None`, not zero and not a hundred.** A vault with no logins
//!   has no health to report, and both numeric answers are lies in opposite
//!   directions.
//! - **Reuse counts participants, not groups.** Three items sharing a password
//!   are three problems, because fixing one leaves two.
//! - **Round once, at the end.** Rounding each term first drifts by up to two
//!   points and makes the visible breakdown fail to add up to the visible score,
//!   which is exactly what "explainable" rules out.
//!
//! What is *not* here: the breach lookup, the strength estimate and the 2FA
//! directory. Those are inputs. Keeping them out is what lets the score be
//! tested exhaustively without a network, a wordlist or a vault.

use std::collections::HashMap;

use uuid::Uuid;

/// The four weights, in the order the breakdown displays them.
const WEIGHT_BREACHED: f64 = 35.0;
const WEIGHT_WEAK: f64 = 25.0;
const WEIGHT_REUSED: f64 = 20.0;
const WEIGHT_TWO_FACTOR: f64 = 20.0;

/// Weights when no item is 2FA-capable, written down so nobody re-derives them.
///
/// SPEC-V1 §7.4 gives these literally: the 2FA weight is redistributed across
/// the other three in proportion, `35 : 25 : 20` scaled by `100/80`.
const WEIGHT_BREACHED_NO_2FA: f64 = 43.75;
const WEIGHT_WEAK_NO_2FA: f64 = 31.25;
const WEIGHT_REUSED_NO_2FA: f64 = 25.0;

/// Counts the score is computed from.
///
/// Deliberately a bag of numbers rather than a view over the vault: every field
/// is something the UI already shows, so the score can be explained by pointing
/// at the same figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HealthInputs {
    /// Live login items. The denominator for the first three terms.
    pub logins: usize,
    /// Items whose password appears in a breach corpus.
    pub breached: usize,
    /// Items whose password is weak.
    pub weak: usize,
    /// Items **participating in** a reuse group, not the number of groups.
    pub reused: usize,
    /// Items whose service is known to support a second factor.
    pub two_factor_capable: usize,
    /// Capable items that have a TOTP configured.
    pub two_factor_enabled: usize,
}

/// One weighted term, so the UI can render the arithmetic rather than assert it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Term {
    /// Weight actually applied, after any redistribution.
    pub weight: f64,
    /// The fraction earned, in `0.0..=1.0`.
    pub fraction: f64,
    /// `weight × fraction`, unrounded.
    pub points: f64,
}

/// The four weighted terms behind a score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Breakdown {
    /// Not-breached term.
    pub breached: Term,
    /// Not-weak term.
    pub weak: Term,
    /// Not-reused term.
    pub reused: Term,
    /// Two-factor term. Weight is zero when nothing is capable.
    pub two_factor: Term,
}

impl Breakdown {
    /// The four terms, in display order.
    #[must_use]
    pub const fn terms(&self) -> [Term; 4] {
        [self.breached, self.weak, self.reused, self.two_factor]
    }
}

/// The score and the arithmetic behind it.
///
/// An enum rather than a struct with two `Option`s, because SPEC-V1 §7.4's
/// `N == 0` case is *not* "a score of zero with an empty breakdown" — it is the
/// absence of both. Modelling it as one variant makes it impossible to render a
/// breakdown next to "not enough data", or a score with nothing behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthScore {
    /// No live login items. §7.4: null, *"not 0, not 100"*.
    NotEnoughData,
    /// A score in 0–100 and the arithmetic that produced it.
    Scored {
        /// The rounded, clamped score.
        score: u8,
        /// The terms it was rounded from.
        breakdown: Breakdown,
    },
}

impl HealthScore {
    /// The score, or `None` for "not enough data".
    ///
    /// This is what crosses IPC: SPEC-V1 §7.4 requires null rather than a
    /// number when there is nothing to score.
    #[must_use]
    pub const fn value(&self) -> Option<u8> {
        match self {
            Self::NotEnoughData => None,
            Self::Scored { score, .. } => Some(*score),
        }
    }

    /// The breakdown, when there is one.
    #[must_use]
    pub const fn breakdown(&self) -> Option<Breakdown> {
        match self {
            Self::NotEnoughData => None,
            Self::Scored { breakdown, .. } => Some(*breakdown),
        }
    }

    /// Whether the breakdown adds up to the score, within rounding.
    ///
    /// AC13 asserts this. It is a method rather than a test helper because a
    /// breakdown that does not add up is a bug the *product* should never ship,
    /// not merely one the test suite should catch.
    #[must_use]
    pub fn breakdown_adds_up(&self) -> bool {
        let Self::Scored { score, breakdown } = self else {
            // Nothing to add up, and nothing rendered beside it.
            return true;
        };
        let total: f64 = breakdown.terms().iter().map(|t| t.points).sum();
        let rounded = total.round().clamp(0.0, 100.0);
        (rounded - f64::from(*score)).abs() < f64::EPSILON
    }
}

/// A count as a float, for the weighted arithmetic.
///
/// `cast_precision_loss` warns about counts above 2^53. These are item counts in
/// a single vault — SPEC-V1 §9 sizes the product at 10,000 items — so the
/// conversion is exact by eleven orders of magnitude.
#[allow(clippy::cast_precision_loss)]
fn as_f64(count: usize) -> f64 {
    count as f64
}

/// A fraction that is 1.0 when the denominator is zero.
///
/// "None of zero items are breached" is a clean sheet, not a division by zero.
fn complement(bad: usize, total: usize) -> f64 {
    if total == 0 {
        return 1.0;
    }
    let ratio = as_f64(bad.min(total)) / as_f64(total);
    1.0 - ratio
}

/// Compute the health score (SPEC-V1 §7.4).
///
/// Returns [`HealthScore::NotEnoughData`] when there are no login items.
#[must_use]
pub fn health(inputs: HealthInputs) -> HealthScore {
    if inputs.logins == 0 {
        return HealthScore::NotEnoughData;
    }
    let has_capable = inputs.two_factor_capable > 0;

    let (w_breached, w_weak, w_reused, w_2fa) = if has_capable {
        (
            WEIGHT_BREACHED,
            WEIGHT_WEAK,
            WEIGHT_REUSED,
            WEIGHT_TWO_FACTOR,
        )
    } else {
        (
            WEIGHT_BREACHED_NO_2FA,
            WEIGHT_WEAK_NO_2FA,
            WEIGHT_REUSED_NO_2FA,
            0.0,
        )
    };

    let term = |weight: f64, fraction: f64| Term {
        weight,
        fraction,
        points: weight * fraction,
    };

    let breached = term(w_breached, complement(inputs.breached, inputs.logins));
    let weak = term(w_weak, complement(inputs.weak, inputs.logins));
    let reused = term(w_reused, complement(inputs.reused, inputs.logins));
    let two_factor = term(
        w_2fa,
        if has_capable {
            let enabled = inputs.two_factor_enabled.min(inputs.two_factor_capable);
            as_f64(enabled) / as_f64(inputs.two_factor_capable)
        } else {
            // No weight, so the fraction is not used. Reported as 1.0 rather
            // than 0.0 so a UI that renders "0 of 0" does not show a red bar for
            // a vault that has nothing to fix.
            1.0
        },
    );

    // Rounded exactly once, here, after redistribution. Rounding each term
    // first drifts by up to two points and makes the visible breakdown fail to
    // add up to the visible score.
    let total: f64 = breached.points + weak.points + reused.points + two_factor.points;
    #[allow(clippy::cast_possible_truncation)]
    let rounded = total.round().clamp(0.0, 100.0) as i64;

    HealthScore::Scored {
        score: u8::try_from(rounded).unwrap_or(100),
        breakdown: Breakdown {
            breached,
            weak,
            reused,
            two_factor,
        },
    }
}

/// A set of items sharing one password (SPEC-V1 §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseGroup {
    /// Every item using this password. Always two or more.
    pub items: Vec<Uuid>,
}

/// Group items by shared password.
///
/// SPEC-V1 §7.4: *"Hash comparison in memory only. Report reuse groups so the
/// user sees what else is affected."* The grouping key never leaves this
/// function and no password is retained: the returned groups carry item ids and
/// nothing else, so the result is safe to hand to the UI.
///
/// Items with an empty password are ignored. A vault full of blank placeholders
/// is not a vault full of reuse, and reporting it as one buries the real cases.
#[must_use]
pub fn reuse_groups(passwords: &[(Uuid, &str)]) -> Vec<ReuseGroup> {
    let mut buckets: HashMap<&str, Vec<Uuid>> = HashMap::new();
    for (id, password) in passwords {
        if password.is_empty() {
            continue;
        }
        buckets.entry(password).or_default().push(*id);
    }

    let mut groups: Vec<ReuseGroup> = buckets
        .into_values()
        .filter(|items| items.len() > 1)
        .map(|mut items| {
            // Sorted so the output is deterministic; a report that reorders
            // itself between runs looks like it changed when it did not.
            items.sort();
            ReuseGroup { items }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.items
            .len()
            .cmp(&a.items.len())
            .then(a.items.cmp(&b.items))
    });
    groups
}

/// How many items participate in any reuse group.
///
/// Three items sharing one password count as three, not one: fixing one of them
/// leaves two still exposed, so three is the number of problems.
#[must_use]
pub fn reused_item_count(groups: &[ReuseGroup]) -> usize {
    groups.iter().map(|g| g.items.len()).sum()
}

// ── Assembling a whole report (SPEC-V1 §7.4) ────────────────────────────────

use crate::services::breach::{self, BreachStatus, RangeSource};
use crate::services::strength::{self, Band, Strength};
use crate::services::twofactor;

/// One item as the report sees it.
///
/// Borrowed throughout: the caller holds the passwords in `Zeroizing` buffers and
/// this type never copies one, so a report over 500 items makes no second copy of
/// any secret.
#[derive(Debug, Clone, Copy)]
pub struct ItemUnderReview<'a> {
    /// Item id.
    pub id: Uuid,
    /// Title, for the risk list.
    pub title: &'a str,
    /// Subtitle, for the risk list.
    pub subtitle: &'a str,
    /// The password. Never stored, never returned.
    pub password: &'a str,
    /// Whether a TOTP configuration exists.
    pub has_totp: bool,
    /// The item’s URLs, reduced to eTLD+1 to ask whether the service takes a
    /// one-time code at all. Never used for anything that reaches the network.
    pub urls: &'a [String],
}

/// Why an item appears in the risk list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskKind {
    /// Found in a breach corpus.
    Breached,
    /// Under §7.4's crack-time threshold.
    Weak,
    /// Shared with at least one other item.
    Reused,
    /// The service accepts an authenticator app and this item has no code.
    MissingTwoFactor,
}

/// One entry in the risk list.
///
/// Carries the item's id and the *shape* of the problem, never the password and
/// never anything derived from it beyond the figures §7.4 asks to be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Risk {
    /// Which item.
    pub item_id: Uuid,
    /// Why.
    pub kind: RiskKind,
    /// Appearances in a breach corpus, when `kind` is `Breached`.
    pub breach_count: Option<u32>,
    /// Estimated offline crack time in seconds, when `kind` is `Weak`.
    pub crack_seconds: Option<u64>,
}

/// A whole report.
#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    /// The counts the score was computed from, so the UI can show its working.
    pub inputs: HealthInputs,
    /// The score and its breakdown.
    pub score: HealthScore,
    /// Every flagged item, most severe first.
    pub risks: Vec<Risk>,
    /// Reuse groups, so the user sees what else is affected (SPEC-V1 §7.4).
    pub groups: Vec<ReuseGroup>,
    /// Items whose breach status could not be determined.
    ///
    /// Reported separately and never folded into `breached` or into a clean sheet:
    /// §7.4 says *"Offline → 'not checked,' never 'safe.'"*
    pub not_checked: usize,
}

/// Run the whole report (SPEC-V1 §7.4).
///
/// `two_factor_capable` is **0** for now, which redistributes the 2FA weight into
/// 43.75 / 31.25 / 25 exactly as §7.4 prescribes. Knowing which services support a
/// second factor needs the bundled directory, and §7.4 makes redistribution
/// permission a precondition for shipping one — `THIRD-PARTY-NOTICES.md` still
/// records it as unverified. Reporting every item as not-capable is the honest
/// stand-in: it neither credits nor penalises anyone for a factor we cannot know
/// about, and the alternative — guessing from the domain — would flag real items
/// on no evidence.
///
/// `breach` should be a cache-only source. AC14 requires a report to make **zero**
/// requests, and [`crate::services::breach::CachedOnly`] makes that structural
/// rather than a promise.
#[must_use]
pub fn assess_all(items: &[ItemUnderReview<'_>], breach: &dyn RangeSource) -> Assessment {
    let groups = reuse_groups(
        &items
            .iter()
            .map(|item| (item.id, item.password))
            .collect::<Vec<_>>(),
    );
    let reused_ids: std::collections::BTreeSet<Uuid> = groups
        .iter()
        .flat_map(|g| g.items.iter().copied())
        .collect();

    let mut risks = Vec::new();
    let mut breached = 0;
    let mut weak = 0;
    let mut not_checked = 0;
    let mut two_factor_enabled = 0;

    let mut two_factor_capable = 0;

    for item in items {
        let capable = twofactor::capable_for(item.urls);
        if capable {
            two_factor_capable += 1;
        }
        if item.has_totp {
            two_factor_enabled += 1;
        } else if capable {
            // Only a service that would accept one. An item with no code on a
            // service that has no second factor is not a risk, it is a fact.
            risks.push(Risk {
                item_id: item.id,
                kind: RiskKind::MissingTwoFactor,
                breach_count: None,
                crack_seconds: None,
            });
        }

        let status = breach::check_one(item.password, breach);
        match status {
            BreachStatus::Breached { count } => {
                breached += 1;
                risks.push(Risk {
                    item_id: item.id,
                    kind: RiskKind::Breached,
                    breach_count: Some(count),
                    crack_seconds: None,
                });
            }
            BreachStatus::NotChecked => not_checked += 1,
            BreachStatus::NotBreached => {}
        }

        let assessed: Strength = strength::assess(item.password, &[item.title, item.subtitle]);
        if assessed.weak {
            weak += 1;
            risks.push(Risk {
                item_id: item.id,
                kind: RiskKind::Weak,
                breach_count: None,
                crack_seconds: Some(assessed.crack_seconds),
            });
        }

        if reused_ids.contains(&item.id) {
            risks.push(Risk {
                item_id: item.id,
                kind: RiskKind::Reused,
                breach_count: None,
                crack_seconds: None,
            });
        }
    }

    // Breached first, then weak, then reused: that is the order in which a user
    // should act, and a list sorted any other way buries the urgent items.
    risks.sort_by_key(|risk| match risk.kind {
        RiskKind::Breached => 0,
        RiskKind::Weak => 1,
        RiskKind::Reused => 2,
        RiskKind::MissingTwoFactor => 3,
    });

    let inputs = HealthInputs {
        logins: items.len(),
        breached,
        weak,
        reused: reused_item_count(&groups),
        two_factor_capable,
        two_factor_enabled,
    };

    Assessment {
        inputs,
        score: health(inputs),
        risks,
        groups,
        not_checked,
    }
}

/// The meter band for one password, for the item detail view (SPEC-V1 §7.2).
///
/// Exposed here so the detail view and the report agree by construction rather
/// than by two callers happening to use the same threshold.
#[must_use]
pub fn band_of(password: &str, context: &[&str]) -> Band {
    strength::assess(password, context).band
}
