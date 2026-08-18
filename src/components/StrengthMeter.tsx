/**
 * Four segments that fill **and collapse** left-to-right with a 60ms stagger.
 *
 * The fill is a `scaleX` transform on an inner element rather than a width change, so it
 * is composited and the collapse mirrors the fill exactly.
 *
 * The design sets the transform, colour and per-segment delay inline. Under the production
 * CSP (`style-src 'self'`) those attributes are dropped and the meter would render
 * permanently empty in a packaged build, so the score is a data attribute and
 * `theme/dynamic.css` carries the rules.
 */

import { cn } from '../lib/cn';

export interface StrengthMeterProps {
  /** 0–4. 0 renders an empty meter, which is the no-password state. */
  score: number;
  /** Accessible summary, e.g. "Strong". Empty when there is nothing to describe. */
  label: string;
  /** Extra classes, for the generator's fixed-width variant. */
  className?: string;
}

export function StrengthMeter({ score, label, className }: StrengthMeterProps) {
  const clamped = Math.max(0, Math.min(4, Math.round(score)));
  return (
    <div
      className={cn('meter flex min-w-0 flex-1 gap-1', className)}
      data-score={clamped}
      role="img"
      aria-label={label === '' ? 'Not scored' : label}
    >
      {[0, 1, 2, 3].map((index) => (
        <div key={index} className="bg-strong h-[5px] flex-1 overflow-hidden rounded-xs">
          <span className="meter__fill block h-full w-full rounded-xs" data-index={index} />
        </div>
      ))}
    </div>
  );
}
