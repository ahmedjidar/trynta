/**
 * Segmented control: one track, one sliding indicator.
 *
 * One track, one sliding indicator. Selection is conveyed by **both** the indicator and a
 * weight change, because the indicator alone is ~1.1:1 against the track
 * (contrast-report finding 4).
 *
 * ## Why the indicator is positioned in CSS
 *
 * The design positions the indicator with an inline `style` computed from the segment count.
 * The production CSP is `style-src 'self'`, so that attribute is dropped in release builds
 * and the indicator would sit at the left edge forever — visible only in a packaged build.
 * The count and index are data attributes instead, and `theme/dynamic.css` carries one
 * rule per position. Two segment counts exist in the product; the file covers four.
 *
 * ## Keyboard
 *
 * `role="tablist"` with arrow-key navigation, which the design's div-based track has no
 * way to express. Left/Right move and activate, matching the pattern for a tablist whose
 * panels are the surface below.
 */

import type { KeyboardEvent, ReactNode } from 'react';

import { cn } from '../lib/cn';

export interface Segment<T extends string> {
  id: T;
  name: string;
  icon?: ReactNode;
}

export interface SegmentedControlProps<T extends string> {
  segments: readonly Segment<T>[];
  value: T;
  onChange: (id: T) => void;
  /** Accessible name for the group. */
  label: string;
  /** `lg` = 34px track (generator, new item), `sm` = 32px. */
  size?: 'lg' | 'sm';
  className?: string;
}

export function SegmentedControl<T extends string>({
  segments,
  value,
  onChange,
  label,
  size = 'lg',
  className,
}: SegmentedControlProps<T>) {
  const index = Math.max(
    0,
    segments.findIndex((s) => s.id === value),
  );

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const delta = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (delta === 0) return;
    event.preventDefault();
    const next = segments[(index + delta + segments.length) % segments.length];
    if (next) onChange(next.id);
  }

  return (
    <div
      role="tablist"
      aria-label={label}
      data-count={segments.length}
      data-index={index}
      onKeyDown={onKeyDown}
      className={cn(
        'segmented bg-surface-hover relative flex shrink-0 rounded-lg p-[3px]',
        size === 'lg' ? 'h-[34px]' : 'h-8',
        className,
      )}
    >
      <span
        aria-hidden="true"
        className="segmented__indicator bg-surface-panel shadow-segment absolute rounded-sm"
      />
      {segments.map((segment) => {
        const active = segment.id === value;
        return (
          <button
            key={segment.id}
            type="button"
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            data-focus-ring
            onClick={() => {
              onChange(segment.id);
            }}
            className={cn(
              'duration-moderate relative flex flex-1 items-center justify-center gap-1.5 rounded-sm transition-colors',
              size === 'lg' ? 'text-control' : 'text-caption',
              active ? 'text-text-primary font-bold' : 'text-text-secondary font-medium',
            )}
          >
            {segment.icon}
            {segment.name}
          </button>
        );
      })}
    </div>
  );
}
