/**
 * The core surface pattern — HO-002 `ui/GroupedList.tsx`.
 *
 * Used by detail fields, generator options, the risk list, settings, generator history and
 * the sheets. Hairlines come from `gap: 1px` over a hairline-coloured background, **not**
 * from per-row borders: that is what guarantees there is never a trailing divider and
 * never a doubled line where two groups meet.
 */

import type { HTMLAttributes, ReactNode } from 'react';

import { cn } from '../lib/cn';

export function GroupedList({ className, children, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn(
        'bg-hairline shadow-card flex flex-col gap-px overflow-hidden rounded-lg',
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}

export interface GroupedRowProps extends HTMLAttributes<HTMLDivElement> {
  /** Adds the hover treatment. Pair with `onClick` or a control inside. */
  interactive?: boolean;
}

export function GroupedRow({ className, interactive, ...props }: GroupedRowProps) {
  return (
    <div
      className={cn(
        'bg-surface-raised flex items-center gap-4 px-4',
        interactive && 'duration-quick hover:bg-surface-hover cursor-pointer transition-colors',
        className,
      )}
      {...props}
    />
  );
}

/** Uppercase micro heading that sits above a group or card. */
export function SectionLabel({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <div
      className={cn(
        'text-micro tracking-label text-text-caption-aa flex h-6 items-end font-bold uppercase',
        className,
      )}
    >
      {children}
    </div>
  );
}

/**
 * Fixed-width label column inside a grouped row.
 *
 * `text-text-caption-aa` rather than HO-002's `text-text-muted`: these are field labels,
 * which are body text, and `--text-muted` fails AA on every surface in both themes
 * (contrast-report finding 2). The alias resolves to `--text-secondary`.
 */
export function FieldLabel({ children }: { children: ReactNode }) {
  return (
    <div className="text-caption text-text-caption-aa w-24 shrink-0 font-medium">{children}</div>
  );
}

/** Rounded card used for Activity, Notes and the settings statements. */
export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('bg-surface-raised shadow-card rounded-lg p-4', className)} {...props} />
  );
}
