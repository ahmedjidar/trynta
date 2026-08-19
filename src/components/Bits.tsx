// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Small controls: `CopyAction`, `Chip`, `Badge` and `Input`.
 *
 * Each is a real `<button>` or `<input>` with a focus ring rather than a `div` with an
 * `onClick`, which is the accessibility gap the design's own notes open with.
 */

import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from 'react';

import { cn } from '../lib/cn';

/** Low-emphasis capsule action: Copy, Reveal, Hide. */
export function CopyAction({ className, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      data-focus-ring
      className={cn(
        'bg-surface-hover flex h-6 shrink-0 items-center gap-0.5 rounded-full px-[11px]',
        'text-chip text-text-secondary font-semibold',
        'duration-quick hover:bg-surface-selected hover:text-accent transition-colors',
        'disabled:hover:bg-surface-hover disabled:hover:text-text-secondary disabled:cursor-default disabled:opacity-45',
        className,
      )}
      {...props}
    />
  );
}

export interface ChipProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Selected chips take the accent fill and accent text. */
  selected?: boolean;
}

/** Filter or vault chip. Selection is fill plus accent text. */
export function Chip({ selected, className, ...props }: ChipProps) {
  return (
    <button
      type="button"
      data-focus-ring
      aria-pressed={selected}
      className={cn(
        'text-chip flex h-6 shrink-0 items-center gap-1.5 rounded-full px-[11px] font-semibold',
        'duration-quick transition-colors',
        selected
          ? 'bg-surface-selected text-accent'
          : 'text-text-secondary hover:bg-surface-hover bg-transparent',
        className,
      )}
      {...props}
    />
  );
}

/** Which semantic ramp a badge or figure reads from. */
export type Tone = 'accent' | 'warning' | 'danger' | 'info' | 'neutral' | 'empty';

/**
 * Status pill: the 2FA marker, risk tags.
 *
 * The foreground comes from the `[data-tone]` rules in `theme/dynamic.css`, so a tone
 * cannot be spelled one way here and another way in the stat cards.
 */
export function Badge({
  tone = 'neutral',
  size = 'md',
  children,
}: {
  /** Semantic ramp. */
  tone?: Tone;
  /** `sm` is the inline 2FA marker; `md` the risk tag; `lg` the roster pill. */
  size?: 'sm' | 'md' | 'lg';
  children: ReactNode;
}) {
  return (
    <span
      data-tone={tone}
      className={cn(
        'inline-flex shrink-0 items-center justify-center rounded-full font-bold',
        size === 'sm' && 'text-badge-xs tracking-badge h-[17px] rounded-xs px-[5px]',
        size === 'md' && 'text-badge h-5 px-[10px]',
        size === 'lg' && 'text-micro h-[22px] px-[11px]',
        tone === 'accent' && 'bg-accent-subtle',
        tone === 'warning' && 'bg-status-warning-subtle',
        tone === 'danger' && 'bg-status-danger-subtle',
        (tone === 'info' || tone === 'neutral' || tone === 'empty') && 'bg-surface-hover',
      )}
    >
      {children}
    </span>
  );
}

/** Text input used inside sheets and inline edit mode. */
export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      // The webview's own autofill must never populate a vault form: it would write the
      // host browser's saved form data into the vault, and it would look typed.
      autoComplete="off"
      autoCorrect="off"
      spellCheck={false}
      className={cn(
        'border-strong bg-surface-panel h-[30px] min-w-0 rounded-md border px-2.5',
        'text-body text-text-primary duration-base transition-[box-shadow,border-color] outline-none',
        className,
      )}
      {...props}
    />
  );
}
