/**
 * Four-up KPI row at the top of the security report.
 *
 * The figure's colour comes from the `[data-tone]` rules in `theme/dynamic.css` rather
 * than an inline style, for the CSP reason in that file's header.
 */

import { cn } from '../lib/cn';
import type { Tone } from './Bits';

/** One card in the KPI row. */
export interface Stat {
  /** Bold label under the figure. */
  label: string;
  /** The figure itself, already formatted. */
  value: string;
  /** One-line explanation of what it counts. */
  sub: string;
  /** Which semantic ramp colours the figure. */
  tone: Tone;
}

export function StatCards({ stats, className }: { stats: readonly Stat[]; className?: string }) {
  return (
    <div className={cn('grid grid-cols-4 gap-3', className)}>
      {stats.map((stat) => (
        <div key={stat.label} className="bg-surface-raised shadow-card rounded-lg p-4">
          <div className="text-stat tracking-metric font-bold tabular-nums" data-tone={stat.tone}>
            {stat.value}
          </div>
          <div className="text-control text-text-primary mt-1 font-semibold">{stat.label}</div>
          <div className="text-chip text-text-muted mt-0.5">{stat.sub}</div>
        </div>
      ))}
    </div>
  );
}
