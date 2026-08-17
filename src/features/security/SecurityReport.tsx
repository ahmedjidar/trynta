/**
 * Security report — components.md §11, SPEC-V1 §7.4.
 *
 * ## The stat cards, and where their colours come from
 *
 * §11 says the four cards *"map to --stat-red/-amber/-violet/-cyan"*. Those tokens live
 * in the design's external design-system file, and their values are exactly Keyring's
 * `--status-danger`, `--status-warning`, `--accent` and `--status-info`. So the cards use
 * Keyring's own names: same appearance, one token layer, and light theme works — the
 * `--stat-*` values are dark-only raw hex with no light override.
 *
 * ## Two things this surface must not do
 *
 * **It makes no network request.** `security_report_run` is handed a cache-only breach
 * source in Rust, so AC14's "zero requests from a report" is structural. Refreshing the
 * cache is a separate command the user triggers.
 *
 * **It never shows an unchecked item as safe.** §7.4: *"Offline → 'not checked,' never
 * 'safe.'"* `notChecked` is reported separately from `breached` and gets its own line,
 * because folding it into either would be the lie the criterion exists to prevent.
 *
 * ## The score
 *
 * `null` means "not enough data" — §7.4: *"not 0, not 100."* The breakdown renders
 * beside it because §7.4 requires the arithmetic to be visible, and the weights come
 * from the response rather than being hardcoded, so shipping the 2FA directory later
 * needs no frontend change.
 */

import { useNavigation } from '../../app/navigation';
import { Button, Group, GroupRow } from '../../components/Controls';
import { Glyph } from '../../components/Glyph';
import { IdentityTile } from '../../components/IdentityTile';
import type { ItemSummaryDto, RiskDto, SecurityReportDto } from '../../ipc';

/** Card tone per stat, mapped onto Keyring's status tokens. */
const CARDS = [
  { key: 'breached', tone: 'danger', label: 'Breached', sub: 'Found in a known credential dump' },
  { key: 'weak', tone: 'warning', label: 'Weak', sub: 'Guessable in under a day' },
  { key: 'reused', tone: 'accent', label: 'Reused', sub: 'Shared across two or more items' },
  { key: 'notChecked', tone: 'info', label: 'Not checked', sub: 'No breach data for these yet' },
] as const;

/** Tag copy and tone per risk kind. */
const TAGS = {
  breached: { label: 'Breached', tone: 'danger' },
  weak: { label: 'Weak', tone: 'warning' },
  reused: { label: 'Reused', tone: 'warning' },
} as const;

export interface SecurityReportProps {
  /** The report, from `security_report_run`. */
  report: SecurityReportDto;
  /** List rows, for the identity tile and subtitle on each risk. */
  items: readonly ItemSummaryDto[];
  /** Refresh the breach cache. The only command here that reaches the network. */
  onCheckNow: () => void;
  /** Whether a refresh is permitted right now (§7.4's 24-hour cadence). */
  canCheck: boolean;
}

export function SecurityReport({ report, items, onCheckNow, canCheck }: SecurityReportProps) {
  const select = useNavigation((s) => s.select);
  const go = useNavigation((s) => s.go);

  const counts: Record<string, number> = {
    breached: report.breached,
    weak: report.weak,
    reused: report.reused,
    notChecked: report.notChecked,
  };

  return (
    <section className="pane pane--wide" aria-label="Security report">
      <div className="pane__content pane__content--wide">
        <header className="security__header">
          <div className="security__intro">
            <h1 className="pane__title">Security report</h1>
            <p className="pane__prose">
              {/* §7.4's own framing, and it is the honest one: only 5-character hash
                  prefixes are ever sent, and never on this screen. */}
              Checked against the Have I Been Pwned corpus using k-anonymous hash prefixes. Your
              passwords never leave this device, and opening this report sends nothing.
            </p>
          </div>
          <div className="security__score">
            <span className="security__score-value">
              {report.score === null ? '—' : report.score}
            </span>
            <span className="security__score-label">Vault health</span>
          </div>
        </header>

        {report.score === null ? (
          <p className="security__empty">
            {/* §7.4: N == 0 is null, "not 0, not 100". Saying so beats a zero that reads
                as a catastrophic score. */}
            Not enough data to score. Add a login and the report will have something to measure.
          </p>
        ) : (
          <>
            <div className="stat-cards">
              {CARDS.map((card) => (
                <article key={card.key} className="stat-card" data-tone={card.tone}>
                  <span className="stat-card__value">{counts[card.key] ?? 0}</span>
                  <span className="stat-card__label">{card.label}</span>
                  <span className="stat-card__sub">{card.sub}</span>
                </article>
              ))}
            </div>

            {report.breakdown === null ? null : (
              <Group label="How the score is calculated">
                {/* §7.4: "breakdown always visible — the user should see why, not just
                    what". The weights come from the response, so the 43.75/31.25/25
                    redistribution while no 2FA directory ships needs no code here. */}
                {[
                  { label: 'Not breached', term: report.breakdown.breached },
                  { label: 'Not weak', term: report.breakdown.weak },
                  { label: 'Not reused', term: report.breakdown.reused },
                  { label: 'Two-factor', term: report.breakdown.twoFactor },
                ].map(({ label, term }) => (
                  <GroupRow key={label}>
                    <span className="field-label field-label--wide">{label}</span>
                    <span className="breakdown__weight">
                      {term.weight === 0
                        ? 'no weight'
                        : `${term.weight.toFixed(2)} × ${(term.fraction * 100).toFixed(0)}%`}
                    </span>
                    <span className="breakdown__points">{term.points.toFixed(2)}</span>
                  </GroupRow>
                ))}
              </Group>
            )}

            {report.twoFactorCapable === 0 ? (
              <p className="security__note">
                {/* Not a footnote. The 2FA term carries no weight in this build and the
                    other three weights are larger as a result, so the score is not
                    comparable with one from a build that ships the directory. */}
                The two-factor term carries no weight in this build: the bundled directory of which
                services support a second factor is not shipped, so nothing can be reported as
                capable. Its 20 points are redistributed across the other three, exactly as §7.4
                specifies.
              </p>
            ) : null}
          </>
        )}

        <div className="security__section-head">
          <h2 className="security__section-title">Needs attention</h2>
          <span className="security__section-count">
            {report.risks.length === 1 ? '1 item' : `${String(report.risks.length)} items`}
          </span>
          <span className="detail-spacer" />
          <Button variant="outline" onClick={onCheckNow} disabled={!canCheck}>
            {canCheck ? 'Check for breaches now' : 'Checked in the last 24 hours'}
          </Button>
        </div>

        {report.risks.length === 0 ? (
          <Group>
            <GroupRow height="risk">
              <span className="security__clear">
                {/* §11's own empty state: "a --accent shield-check and 'Every password is
                    strong.'" Reworded because with `notChecked` above zero, "every" would
                    be a claim the data does not support. */}
                <Glyph name="verified" />
                {report.notChecked > 0
                  ? 'Nothing flagged among the passwords that could be checked.'
                  : 'Every password is strong.'}
              </span>
            </GroupRow>
          </Group>
        ) : (
          <Group>
            {report.risks.map((risk) => (
              <RiskRow
                key={`${risk.itemId}-${risk.kind}`}
                risk={risk}
                item={items.find((i) => i.id === risk.itemId)}
                onOpen={() => {
                  select(risk.itemId);
                  go('vault');
                }}
              />
            ))}
          </Group>
        )}
      </div>
    </section>
  );
}

interface RiskRowProps {
  risk: RiskDto;
  item: ItemSummaryDto | undefined;
  onOpen: () => void;
}

function RiskRow({ risk, item, onOpen }: RiskRowProps) {
  const tag = TAGS[risk.kind];
  const detail =
    risk.kind === 'breached' && risk.breachCount !== null
      ? `Seen ${risk.breachCount.toLocaleString()} times in breach data`
      : risk.kind === 'weak' && risk.crackSeconds !== null
        ? `Crackable in about ${describeSeconds(risk.crackSeconds)}`
        : (risk.subtitle ?? '');

  return (
    <GroupRow height="risk" onClick={onOpen} label={`Open ${risk.title}`}>
      {item ? (
        <IdentityTile icon={item.icon} title={risk.title} />
      ) : (
        <span className="tile tile--md" aria-hidden="true" />
      )}
      <span className="risk__labels">
        <span className="risk__name">{risk.title}</span>
        <span className="risk__sub">{detail}</span>
      </span>
      <span className="risk__tag-column">
        <span className="risk-tag" data-tone={tag.tone}>
          {tag.label}
        </span>
      </span>
      <span className="risk__fix">
        Fix
        <Glyph name="next" />
      </span>
    </GroupRow>
  );
}

/** A rough human duration. §7.4 asks for the estimate to be shown, not to be precise. */
function describeSeconds(seconds: number): string {
  if (seconds < 60) return 'seconds';
  if (seconds < 3600) return `${String(Math.round(seconds / 60))} minutes`;
  if (seconds < 86_400) return `${String(Math.round(seconds / 3600))} hours`;
  return `${String(Math.round(seconds / 86_400))} days`;
}
