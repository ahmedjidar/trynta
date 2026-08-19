// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Capsule button.
 *
 * Two variants only: the design has no tertiary or ghost button, and low-emphasis actions
 * use `CopyAction` or `Chip` instead.
 *
 * Disabled is a **token swap, not opacity**, because the gated Save button in the new-item
 * sheet has to stay legible while inert — an opacity fade on a footer button reads as a
 * rendering fault rather than as "not yet".
 */

import type { ButtonHTMLAttributes } from 'react';

import { cn } from '../lib/cn';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** `primary` is the accent fill; `outline` is the panel fill with a border. */
  variant?: 'primary' | 'outline';
  /** Fills its container, as the lock screen's buttons do. */
  block?: boolean;
}

export function Button({ variant = 'primary', block, className, ...props }: ButtonProps) {
  return (
    <button
      type="button"
      data-focus-ring
      className={cn(
        'text-control flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-xl font-semibold',
        'duration-base transition-[background-color,box-shadow,transform] active:scale-[.97]',
        'disabled:cursor-default disabled:active:scale-100',
        variant === 'primary' && [
          'bg-accent text-text-on-accent shadow-accent-glow px-[17px]',
          'enabled:hover:bg-accent-hover',
          'disabled:bg-surface-hover disabled:text-text-muted disabled:shadow-none',
        ],
        variant === 'outline' && [
          'border-strong bg-surface-panel text-text-primary border px-[15px]',
          'enabled:hover:bg-surface-hover',
          'disabled:opacity-45',
        ],
        block && 'w-full',
        className,
      )}
      {...props}
    />
  );
}
