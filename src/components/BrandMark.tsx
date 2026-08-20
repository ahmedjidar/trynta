// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The Trynta mark: two interlocking rings.
 *
 * **Ours, and deliberately nowhere near the third-party icons.** `public/icons/` is
 * 3,952 brand marks belonging to other companies, resolved from a domain map; this
 * is the product's own identity and lives under `public/trynta/brand/`. Nothing maps
 * a domain to it and nothing ever should — a Trynta mark appearing on a vault item
 * would be claiming the user has an account with us.
 *
 * ## Why it is inline rather than an `<img>`
 *
 * The brand icons are `<img src="/icons/…">` because there are thousands of them and
 * they are data. There is one of these, it is drawn at 16–28px where a raster falls
 * apart, and it has to take its colour from the theme. Inline SVG gives it
 * `currentColor`; an `<img>` cannot inherit colour at all.
 *
 * ## Colour
 *
 * `currentColor`, set by whatever renders it, and the only two values it is ever
 * given are the supplied variants: the soft light blue-violet on dark, the dark
 * blue-black on light. `--brand-mark` in `theme/dynamic.css` picks between them. The
 * mark is never recoloured beyond that choice.
 *
 * The geometry here is the same construction as
 * `public/trynta/brand/trynta-mark.svg`, which is the source of truth and carries the
 * measurements it was rebuilt from. Kept in step by hand: there is one shape and it
 * has not changed since it was drawn.
 */

export interface BrandMarkProps {
  /** Rendered width in px. Height follows the 122:76 aspect. */
  size?: number;
  /** Extra classes, for spacing at the call site. */
  className?: string;
}

/**
 * The Trynta mark, inheriting its colour from `currentColor`.
 *
 * @param props - See {@link BrandMarkProps}.
 */
export function BrandMark({ size = 20, className }: BrandMarkProps) {
  return (
    <svg
      viewBox="0 0 122 76"
      width={size}
      height={(size * 76) / 122}
      className={className}
      role="img"
      aria-label="Trynta"
      focusable="false"
    >
      {/* Two open arcs, each cut where the other passes over it: the left ring at
          the lower crossing, the right at the upper. Arcs rather than masked
          circles because a mask needs black-and-white channel values that are not
          colours but look exactly like colours to `check:tokens` — and because a
          mask is the first thing to go wrong when a rasteriser flattens this to a
          16px icon frame. See the SVG beside the PNGs for the geometry. */}
      <g fill="none" stroke="currentColor" strokeWidth="11">
        <path d="M 53.38 66.63 A 32.5 32.5 0 1 1 66.63 53.38" />
        <path d="M 68.62 9.37 A 32.5 32.5 0 1 1 55.37 22.62" />
      </g>
    </svg>
  );
}
