/**
 * Small controls — HO-002 `ui/Bits.tsx`.
 *
 * `CopyAction`, `Chip`, `Badge` and `Input`. Each is a real `<button>` or `<input>` with a
 * focus ring, which closes the "div, needs button + ring" gap components.md records against
 * the HTML original.
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

export type Tone = 'accent' | 'warning' | 'danger' | 'info' | 'neutral';

/**
 * Status pill: 2FA, risk tags.
 *
 * The foreground comes from `dynamic.css`'s `[data-tone]` rules, which use the a11y.css
 * `-text` aliases. HO-002 pairs the raw status colour with its subtle fill; that pair is
 * below AA on light surfaces (contrast-report findings 6 and 7).
 */
export function Badge({
  tone = 'neutral',
  size = 'md',
  children,
}: {
  tone?: Tone;
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
        tone === 'info' && 'bg-surface-hover',
        tone === 'neutral' && 'bg-surface-hover',
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
