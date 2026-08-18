/**
 * Security report — SPEC-V1 §7.4.
 *
 * ## The stat cards
 *
 * The four figures take `--status-danger`, `--status-warning`, `--accent` and
 * `--status-info`, resolved through the `[data-tone]` rules in `theme/dynamic.css` rather
 * than inline, for the CSP reason in that file's header.
 *
 * ## Two things this surface must not do
 *
 * **It makes no network request.** `security_report_run` is handed a cache-only breach
 * source in Rust, so AC14's "zero requests from a report" is structural rather than a
 * promise this component keeps. Refreshing the cache is a separate command the user
 * triggers, and the design's "Change all with autofill" is not it — autofill is V3, so
 * the action here is the breach check, which is the one thing that can actually run.
 *
 * **It never shows an unchecked item as safe.** §7.4: *"Offline → 'not checked,' never
 * 'safe.'"* `notChecked` is reported separately and gets its own card, because folding it
 * into either of the others would be the lie the criterion exists to prevent.
 *
 * ## The score
 *
 * The design prints a fixed 82. `null` means "not enough data" — §7.4: *"not 0, not 100"* — and
 * the breakdown renders below it because §7.4 requires the arithmetic to be visible. The
 * weights come from the response, so shipping the 2FA directory later needs no change here.
 */

import { Button } from '../../components/Button';
import { Badge } from '../../components/Bits';
import { GroupedList, GroupedRow } from '../../components/GroupedList';
import { Glyph } from '../../components/Glyph';
import { IdentityTile } from '../../components/IdentityTile';
import { StatCards } from '../../components/StatCards';
import type { Stat } from '../../components/StatCards';
import { useNavigation } from '../../app/navigation';
import type { ItemSummaryDto, RiskDto, SecurityReportDto } from '../../ipc';

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
  /** Refresh the breach cache. The only thing here that reaches the network. */
  onCheckNow: () => void;
  /** Whether a refresh is permitted right now (§7.4's 24-hour cadence). */
  canCheck: boolean;
}

export function SecurityReport({ report, items, onCheckNow, canCheck }: SecurityReportProps) {
  const select = useNavigation((s) => s.select);
  const go = useNavigation((s) => s.go);

  const stats: readonly Stat[] = [
    {
      label: 'Breached',
      value: String(report.breached),
      sub: 'Found in a known credential dump',
      tone: 'danger',
    },
    { label: 'Weak', value: String(report.weak), sub: 'Guessable in under a day', tone: 'warning' },
    {
      label: 'Reused',
      value: String(report.reused),
      sub: 'Shared across two or more items',
      tone: 'accent',
    },
    {
      label: 'Not checked',
      value: String(report.notChecked),
      sub: 'No breach data for these yet',
      tone: 'info',
    },
  ];

  return (
    <section
      data-scroll-pane
      className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
      aria-label="Security report"
    >
      <div className="mx-auto w-full max-w-[var(--measure-pane-wide)] px-10 pt-8 pb-12">
        <header className="flex items-start gap-8">
          <div className="min-w-0 flex-1">
            <h1 className="text-display tracking-display font-bold">Security report</h1>
            <p className="text-body text-text-muted mt-1 max-w-[62ch] leading-5 text-pretty">
              {/* §7.4's own framing, and the honest one: only 5-character hash prefixes
                  are ever sent, and never from this screen. */}
              Checked against the Have I Been Pwned corpus using k-anonymous hash prefixes. Your
              passwords never leave this device, and opening this report sends nothing.
            </p>
          </div>
          <div className="shrink-0 text-right">
            <div
              className="text-metric tracking-metric font-bold tabular-nums"
              data-tone={report.score === null ? 'empty' : 'accent'}
            >
              {report.score === null ? '—' : report.score}
            </div>
            <div className="text-micro tracking-label text-text-muted mt-1.5 font-bold uppercase">
              Vault health
            </div>
          </div>
        </header>

        {report.score === null ? (
          <p className="text-body text-text-muted mt-6">
            {/* §7.4: N == 0 is null, "not 0, not 100". Saying so beats a zero that reads as
                a catastrophic score. */}
            Not enough data to score. Add a login and the report will have something to measure.
          </p>
        ) : (
          <>
            <StatCards className="mt-6" stats={stats} />

            {report.breakdown === null ? null : (
              <>
                <h2 className="text-micro tracking-label text-text-muted mt-8 flex h-6 items-end font-bold uppercase">
                  How the score is calculated
                </h2>
                <GroupedList className="mt-2">
                  {/* §7.4: "breakdown always visible — the user should see why, not just
                      what". The weights come from the response, so the 43.75/31.25/25
                      redistribution while no 2FA directory ships needs no code here. */}
                  {[
                    { label: 'Not breached', term: report.breakdown.breached },
                    { label: 'Not weak', term: report.breakdown.weak },
                    { label: 'Not reused', term: report.breakdown.reused },
                    { label: 'Two-factor', term: report.breakdown.twoFactor },
                  ].map(({ label, term }) => (
                    <GroupedRow key={label} className="h-12">
                      <span className="text-body min-w-0 flex-1 font-medium">{label}</span>
                      <span className="text-caption text-text-muted shrink-0 tabular-nums">
                        {term.weight === 0
                          ? 'no weight'
                          : `${term.weight.toFixed(2)} × ${(term.fraction * 100).toFixed(0)}%`}
                      </span>
                      <span className="text-body w-14 shrink-0 text-right font-bold tabular-nums">
                        {term.points.toFixed(2)}
                      </span>
                    </GroupedRow>
                  ))}
                </GroupedList>
              </>
            )}

            {report.twoFactorCapable === 0 ? (
              <p className="text-chip text-text-muted mt-3 max-w-[62ch] leading-4 text-pretty">
                {/* Not a footnote. The 2FA term carries no weight in this build and the
                    other three are larger as a result, so this score is not comparable
                    with one from a build that ships the directory. */}
                The two-factor term carries no weight in this build: the bundled directory of which
                services support a second factor is not shipped, so nothing can be reported as
                capable. Its 20 points are redistributed across the other three, exactly as §7.4
                specifies.
              </p>
            ) : null}
          </>
        )}

        <div className="mt-8 flex h-8 items-center gap-2.5">
          <h2 className="text-heading tracking-title font-bold">Needs attention</h2>
          <span className="text-caption text-text-muted tabular-nums">
            {report.risks.length === 1 ? '1 item' : `${String(report.risks.length)} items`}
          </span>
          <div className="flex-1" />
          {/* The design's button is "Change all with autofill". Autofill is V3 and bulk
              rotation is out of scope (§7.4), so the action is the one thing this surface
              can actually do. */}
          <Button variant="outline" onClick={onCheckNow} disabled={!canCheck}>
            {canCheck ? 'Check for breaches now' : 'Checked in the last 24 hours'}
          </Button>
        </div>

        {report.risks.length === 0 ? (
          <GroupedList className="mt-3">
            <GroupedRow className="h-14">
              <span className="text-body text-text-secondary flex items-center gap-2">
                {/* components.md specifies an accent shield-check and "Every password is
                    strong." Reworded when `notChecked` is above zero, because "every"
                    would then be a claim the data does not support. */}
                <span className="text-accent">
                  <Glyph name="verified" />
                </span>
                {report.notChecked > 0
                  ? 'Nothing flagged among the passwords that could be checked.'
                  : 'Every password is strong.'}
              </span>
            </GroupedRow>
          </GroupedList>
        ) : (
          <GroupedList className="mt-3">
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
          </GroupedList>
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
    <GroupedRow
      interactive
      className="h-14 gap-3"
      onClick={onOpen}
      role="button"
      tabIndex={0}
      data-focus-ring
      aria-label={`Open ${risk.title}`}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onOpen();
        }
      }}
    >
      {item ? (
        <IdentityTile icon={item.icon} title={risk.title} />
      ) : (
        <span className="tile" data-size="32" data-tone="0" aria-hidden="true" />
      )}
      <div className="min-w-0 flex-1">
        <div className="text-body font-semibold">{risk.title}</div>
        <div className="text-chip text-text-muted truncate">{detail}</div>
      </div>
      <div className="flex w-[84px] shrink-0 justify-end">
        <Badge tone={tag.tone}>{tag.label}</Badge>
      </div>
      <span className="text-caption text-accent flex shrink-0 items-center gap-0.5 font-semibold">
        Fix
        <Glyph name="next" size={14} />
      </span>
    </GroupedRow>
  );
}

/** A rough human duration. §7.4 asks for the estimate to be shown, not to be precise. */
function describeSeconds(seconds: number): string {
  if (seconds < 60) return 'seconds';
  if (seconds < 3600) return `${String(Math.round(seconds / 60))} minutes`;
  if (seconds < 86_400) return `${String(Math.round(seconds / 3600))} hours`;
  return `${String(Math.round(seconds / 86_400))} days`;
}
