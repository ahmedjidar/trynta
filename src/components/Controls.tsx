/**
 * Shared controls — components.md §5, §7, §8, §9.
 *
 * The design mounts these from an external design-system bundle belonging to an
 * unrelated product. They are reimplemented here under Keyring's own names, from the
 * behaviour and token values that bundle applies; nothing is imported from it and no
 * identifier from it appears in this repository.
 *
 * Each one closes an accessibility gap the handoff lists. In the prototype the switch,
 * the segmented control and the copy actions are all `div`s with `onClick`: no role, no
 * keyboard, no focus ring. Here they are a `role="switch"`, a `role="radiogroup"` with
 * arrow-key traversal, and real buttons.
 */

import { useId } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';

// ── Button — §7 ─────────────────────────────────────────────────────────────

export interface ButtonProps {
  children: ReactNode;
  /** `primary` is the accent fill; `outline` is the panel fill with a border. */
  variant?: 'primary' | 'outline';
  onClick?: () => void;
  disabled?: boolean;
  /** Fills the width of its container, as the lock screen's buttons do. */
  block?: boolean;
  type?: 'button' | 'submit';
}

export function Button({
  children,
  variant = 'primary',
  onClick,
  disabled = false,
  block = false,
  type = 'button',
}: ButtonProps) {
  return (
    <button
      type={type}
      className={`btn btn--${variant}`}
      data-block={block || undefined}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

// ── Switch — §8 ─────────────────────────────────────────────────────────────

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** Accessible name. Required: a bare track says nothing to a screen reader. */
  label: string;
  disabled?: boolean;
}

export function Switch({ checked, onChange, label, disabled = false }: SwitchProps) {
  return (
    <button
      type="button"
      // The handoff's gap: "needs role=switch + aria-checked + the focus ring on the
      // track". A native checkbox cannot carry the design's track-and-knob geometry
      // without being visually hidden and re-drawn, which is the same amount of ARIA
      // with a hidden input to keep in sync as well.
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="switch"
      data-on={checked || undefined}
      disabled={disabled}
      onClick={() => {
        onChange(!checked);
      }}
    >
      <span className="switch__knob" />
    </button>
  );
}

// ── Segmented control — §9 ──────────────────────────────────────────────────

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  glyph?: ReactNode;
}

export interface SegmentedProps<T extends string> {
  options: readonly SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  /** Accessible name for the group. */
  label: string;
}

export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: SegmentedProps<T>) {
  const at = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  );
  const count = options.length;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // The handoff's gap: "needs role=tablist/radiogroup with arrow-key traversal".
    // Radio semantics rather than tabs, because these choose a value rather than
    // reveal a panel — the generator's type, the new-item kind.
    const delta =
      event.key === 'ArrowRight' || event.key === 'ArrowDown'
        ? 1
        : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
          ? -1
          : 0;
    if (delta === 0) return;
    event.preventDefault();
    const next = options[(at + delta + count) % count];
    if (next) onChange(next.value);
  };

  return (
    <div
      className="segmented"
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      data-count={count}
    >
      {/* The sliding pill. Its offset is arithmetic over the option count, so it
          cannot be a design token; it is set from a data attribute the stylesheet
          reads, which keeps it out of the banned `style` prop. */}
      <span className="segmented__indicator" data-at={at} aria-hidden="true" />
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            className="segmented__option"
            data-selected={selected || undefined}
            // One tab stop for the group, arrow keys within it.
            tabIndex={selected ? 0 : -1}
            onClick={() => {
              onChange(option.value);
            }}
          >
            {option.glyph}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

// ── Grouped list — §5, the core surface pattern ─────────────────────────────

export interface GroupProps {
  children: ReactNode;
  /** Optional uppercase section heading above the group. */
  label?: string;
}

/**
 * A grouped list.
 *
 * §5: the hairlines are the group's own background showing through a 1px flex gap,
 * *"so there is never a trailing divider"*. That is the whole trick and it is why rows
 * must not carry their own borders.
 */
export function Group({ children, label }: GroupProps) {
  const id = useId();
  return (
    <>
      {label === undefined ? null : (
        <h2 className="group-label" id={id}>
          {label}
        </h2>
      )}
      <div className="group" aria-labelledby={label === undefined ? undefined : id}>
        {children}
      </div>
    </>
  );
}

export interface GroupRowProps {
  children: ReactNode;
  /** Which row-height token applies. */
  height?: 'field' | 'option' | 'risk' | 'setting' | 'history';
  /** Makes the row a button. Omit for a presentational row. */
  onClick?: () => void;
  /** Accessible name when the row is interactive and its text is not enough. */
  label?: string;
}

export function GroupRow({ children, height = 'field', onClick, label }: GroupRowProps) {
  if (onClick) {
    return (
      <button
        type="button"
        className="group-row group-row--interactive"
        data-height={height}
        onClick={onClick}
        aria-label={label}
      >
        {children}
      </button>
    );
  }
  return (
    <div className="group-row" data-height={height}>
      {children}
    </div>
  );
}

// ── Copy action — §6 ────────────────────────────────────────────────────────

export interface CopyActionProps {
  children: ReactNode;
  onClick: () => void;
  /** Accessible name, since "Copy" alone does not say what is copied. */
  label: string;
  disabled?: boolean;
}

export function CopyAction({ children, onClick, label, disabled = false }: CopyActionProps) {
  return (
    <button
      type="button"
      className="copy-action"
      onClick={onClick}
      aria-label={label}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

// ── Strength meter — §6 ─────────────────────────────────────────────────────

export interface StrengthMeterProps {
  /** 0–4. 0 renders an empty meter, which is the no-password state. */
  filled: number;
  /** Accessible summary, e.g. "Strong". */
  label: string;
}

/**
 * Four segments that fill left to right, staggered.
 *
 * The stagger is `calc(var(--stagger-meter) * index)` in the design. Index reaches CSS
 * as a data attribute rather than an inline custom property, because the `style` prop
 * is banned (§7.6) and a fixed four-segment meter needs exactly four rules.
 */
export function StrengthMeter({ filled, label }: StrengthMeterProps) {
  return (
    <div
      className="meter"
      role="img"
      aria-label={label}
      // The band, so the fill colour maps onto `--strength-1..4`.
      data-band={Math.max(0, Math.min(4, filled))}
    >
      {[0, 1, 2, 3].map((index) => (
        <span key={index} className="meter__segment" data-index={index}>
          <span className="meter__fill" data-on={index < filled || undefined} />
        </span>
      ))}
    </div>
  );
}
