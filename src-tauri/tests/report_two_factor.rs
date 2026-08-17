//! The report over a real vault, with no 2FA directory (SPEC-V1 §7.4, AC13, AC14).
//!
//! `report_score.rs` checks the arithmetic against hand-built [`HealthInputs`].
//! This file checks the thing that actually ships: real items in a real encrypted
//! vault, passwords read back through `item_secret`, and the weights that come out
//! the far end.
//!
//! The specific behaviour under test is the one §7.4 wrote the numbers down for.
//! Keyring does not ship the bundled 2FA directory yet — its licence is still
//! unverified in `THIRD-PARTY-NOTICES.md` — so nothing can be reported as
//! *capable* of a second factor, `2fa_capable` is 0, and the 20-point term
//! redistributes into 43.75 / 31.25 / 25. Getting that wrong is not a visible bug:
//! the score still looks like a plausible number. It is only wrong by a few points,
//! in a direction that flatters the vault, on every install. So it is pinned here
//! with the arithmetic written out.
//!
//! The report also makes **zero** network requests (AC14). That is not asserted by
//! watching for one — it is guaranteed by construction, because the only source
//! this test or the real command hands to `assess_all` is
//! [`CachedOnly`](keyring_lib::services::breach::CachedOnly), which has no
//! transport. The breach hit below comes from a cache seeded in-process.

use keyring_lib::services::breach::{self, BreachCache, CachedOnly};
use keyring_lib::services::report::{self, HealthScore, ItemUnderReview, RiskKind};
use keyring_store::{
    ItemBody, ItemDraft, ItemKind, KdfParams, SecretField, TotpAlgorithm, TotpConfig, VaultFile,
};
use zeroize::Zeroizing;

const MASTER: &str = "report-test-master-7Rk2Wp";

/// Strong, unrelated to any title or username, and shared by two items so the
/// reuse term has something to bite on without also tripping the weak term.
const SHARED: &str = "Trombone-Halibut-97-Quasar-Vellum";
/// Weak by §7.4's definition, and the one we seed a breach range for.
const WEAK: &str = "password123";
/// Strong and unique.
const UNIQUE: &str = "Kestrel-Marzipan-42-Obsidian-Rift";

/// A range body for `password`, shaped the way HIBP shapes one.
///
/// Includes a count-0 line, because `Add-Padding: true` means every real response
/// carries them and a parser that counted one as a hit would report a password as
/// breached *because* we asked for privacy.
fn seeded_cache(password: &str, count: u32, now_ms: i64) -> BreachCache {
    let (prefix, suffix) = breach::split(password);
    let body = format!(
        "0000000000000000000000000000000000000:0\n{}:{count}\n",
        suffix.as_str()
    );
    let mut cache = BreachCache::default();
    cache.put(prefix, body, now_ms);
    cache
}

struct Reviewed {
    rows: Vec<(uuid::Uuid, String, String, bool)>,
    passwords: Vec<Zeroizing<String>>,
}

impl Reviewed {
    fn as_items(&self) -> Vec<ItemUnderReview<'_>> {
        self.rows
            .iter()
            .zip(&self.passwords)
            .map(|((id, title, subtitle, has_totp), password)| ItemUnderReview {
                id: *id,
                title,
                subtitle,
                password: password.as_str(),
                has_totp: *has_totp,
            })
            .collect()
    }
}

/// Build a real vault, then read it back exactly the way `security_report_run`
/// does: `index_rows()` for metadata, one `item_secret` per login.
fn build_vault(dir: &std::path::Path) -> Reviewed {
    let path = dir.join("vault.db");
    let file = VaultFile::create(&path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault = session.vault_add("Personal", "vault.accent.1").expect("vault");

    // One login with a TOTP configured. It must still get no 2FA credit, because
    // "capable" is what the missing directory would have told us and we do not know.
    let mut with_totp = ItemDraft::new(
        vault,
        "Acme Corp",
        ItemBody::Login {
            username: "alice".into(),
            password: UNIQUE.into(),
            urls: vec!["https://acme.test".into()],
            totp: Some(TotpConfig {
                secret: "JBSWY3DPEHPK3PXP".into(),
                algorithm: TotpAlgorithm::Sha1,
                digits: 6,
                period_seconds: 30,
                issuer: "Acme".into(),
                account: "alice".into(),
            }),
        },
    );
    with_totp.tags = vec!["work".into()];
    session.item_upsert(&with_totp).expect("upsert");

    for (title, username, password) in [
        ("Bank", "alice.b", WEAK),
        ("Shop North", "alice.n", SHARED),
        ("Shop South", "alice.s", SHARED),
    ] {
        session
            .item_upsert(&ItemDraft::new(
                vault,
                title,
                ItemBody::Login {
                    username: username.into(),
                    password: password.into(),
                    urls: Vec::new(),
                    totp: None,
                },
            ))
            .expect("upsert");
    }

    // A non-login, to prove the denominator counts logins only.
    session
        .item_upsert(&ItemDraft::new(vault, "Passport", ItemBody::SecureNote))
        .expect("upsert");

    let mut rows = Vec::new();
    let mut passwords = Vec::new();
    for row in session.index_rows().expect("index") {
        if row.kind != ItemKind::Login {
            continue;
        }
        let password = session
            .item_secret(row.id, SecretField::Password)
            .expect("password");
        rows.push((
            row.id,
            row.title.clone(),
            row.subtitle.clone().unwrap_or_default(),
            row.has_totp,
        ));
        passwords.push(password);
    }
    Reviewed { rows, passwords }
}

#[test]
fn no_directory_means_zero_capable_and_redistributed_weights() {
    let dir = tempfile::tempdir().expect("tempdir");
    let reviewed = build_vault(dir.path());
    let items = reviewed.as_items();
    assert_eq!(items.len(), 4, "four logins, and the note is not one of them");

    let cache = seeded_cache(WEAK, 4_912_313, 1_700_000_000_000);
    let assessment = report::assess_all(&items, &CachedOnly { cache: &cache });

    // ── The inputs the score was computed from ──
    let inputs = assessment.inputs;
    assert_eq!(inputs.logins, 4);
    assert_eq!(inputs.breached, 1, "only the seeded prefix is in the cache");
    assert_eq!(inputs.weak, 1, "only {WEAK} is under the threshold");
    assert_eq!(inputs.reused, 2, "both shops, not one group");
    assert_eq!(
        inputs.two_factor_capable, 0,
        "no bundled directory ships, so nothing can be reported as capable"
    );
    assert_eq!(
        inputs.two_factor_enabled, 1,
        "the TOTP is still counted — it just earns nothing while capable is 0"
    );
    assert_eq!(
        assessment.not_checked, 3,
        "three logins have no cached range, and 'not checked' is never 'safe'"
    );

    // ── The redistribution, written out ──
    let HealthScore::Scored { score, breakdown } = assessment.score else {
        panic!("four logins is enough data to score");
    };

    assert!(
        (breakdown.breached.weight - 43.75).abs() < f64::EPSILON,
        "breached weight redistributes to 43.75, got {}",
        breakdown.breached.weight
    );
    assert!(
        (breakdown.weak.weight - 31.25).abs() < f64::EPSILON,
        "weak weight redistributes to 31.25, got {}",
        breakdown.weak.weight
    );
    assert!(
        (breakdown.reused.weight - 25.0).abs() < f64::EPSILON,
        "reused weight redistributes to 25, got {}",
        breakdown.reused.weight
    );
    assert!(
        breakdown.two_factor.weight.abs() < f64::EPSILON
            && breakdown.two_factor.points.abs() < f64::EPSILON,
        "the 2FA term carries no weight and contributes no points while capable is 0"
    );

    let total: f64 = breakdown.terms().iter().map(|t| t.weight).sum();
    assert!(
        (total - 100.0).abs() < 1e-9,
        "redistribution must still add to 100, got {total}"
    );

    // 43.75×(1 − 1/4) + 31.25×(1 − 1/4) + 25×(1 − 2/4)
    //   = 32.8125 + 23.4375 + 12.5 = 68.75, rounded once at the end.
    let expected: f64 = 43.75 * 0.75 + 31.25 * 0.75 + 25.0 * 0.5;
    assert!((expected - 68.75).abs() < 1e-9, "the arithmetic above");
    assert_eq!(
        score, 69,
        "68.75 rounds once, at the end, after redistribution"
    );
    assert!(assessment.score.breakdown_adds_up());

    // ── What the user is shown ──
    let breached: Vec<_> = assessment
        .risks
        .iter()
        .filter(|r| r.kind == RiskKind::Breached)
        .collect();
    assert_eq!(breached.len(), 1);
    assert_eq!(
        breached[0].breach_count,
        Some(4_912_313),
        "the count is shown; the password is not"
    );

    let weak: Vec<_> = assessment
        .risks
        .iter()
        .filter(|r| r.kind == RiskKind::Weak)
        .collect();
    assert_eq!(weak.len(), 1);
    assert!(
        weak[0].crack_seconds.is_some_and(|s| s < 86_400),
        "§7.4 defines weak as under a day, and asks for the estimate to be shown"
    );

    assert_eq!(assessment.groups.len(), 1, "one shared password, one group");
    assert_eq!(assessment.groups[0].items.len(), 2);

    // Breached before weak before reused: the order the user should act in.
    let order: Vec<RiskKind> = assessment.risks.iter().map(|r| r.kind).collect();
    let mut sorted = order.clone();
    sorted.sort_by_key(|k| match k {
        RiskKind::Breached => 0,
        RiskKind::Weak => 1,
        RiskKind::Reused => 2,
    });
    assert_eq!(order, sorted, "risks come back most-urgent first");
}

#[test]
fn an_empty_vault_scores_null_not_zero() {
    let cache = BreachCache::default();
    let assessment = report::assess_all(&[], &CachedOnly { cache: &cache });
    assert_eq!(assessment.score, HealthScore::NotEnoughData);
    assert_eq!(assessment.score.value(), None, "§7.4: null, not 0, not 100");
    assert_eq!(assessment.inputs.two_factor_capable, 0);
    assert!(assessment.risks.is_empty());
}
