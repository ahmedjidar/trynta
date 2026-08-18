/**
 * iOS switch — HO-002 `ui/Switch.tsx`.
 *
 * 40×24 track, 20px knob, 2px→18px travel on the Apple spring curve. The knob is white in
 * both themes and relies on `--shadow-knob` for separation, which is contrast-report
 * finding 3 — `a11y.css` adds a border in forced-colors mode so the state survives when
 * the shadow does not.
 */

import type { MouseEvent } from 'react';

import { cn } from '../lib/cn';

export interface SwitchProps {
  checked: boolean;
  onChange: () => void;
  /** Accessible name. Required: the track carries no text of its own. */
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      data-focus-ring
      onClick={(event: MouseEvent) => {
        // The whole row is clickable in the settings and generator lists, so without this
        // a tap on the switch toggles twice and lands back where it started.
        event.stopPropagation();
        onChange();
      }}
      className={cn(
        'duration-slow relative h-6 w-10 shrink-0 rounded-full transition-colors',
        checked ? 'bg-accent' : 'bg-strong',
        disabled && 'opacity-45',
      )}
    >
      <span
        className={cn(
          'bg-surface-knob shadow-knob absolute top-0.5 h-5 w-5 rounded-full',
          'duration-slow ease-spring transition-[left]',
          checked ? 'left-[18px]' : 'left-0.5',
        )}
      />
    </button>
  );
}
