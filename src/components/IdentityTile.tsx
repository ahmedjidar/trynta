/**
 * The tile beside every item: a brand mark, the user's own icon, or a generated shape.
 *
 * The three cases come from Rust as a closed enum, already decided — see
 * `services/icons.rs`. This component draws them and makes no resolution decisions of
 * its own, which is what keeps the rule below true.
 *
 * ## Nothing here can construct a URL
 *
 * ADD-001's central rule is that the app must never fetch an icon, because a favicon
 * request per item transmits an inventory of the user's accounts. The design prototype
 * does exactly that. What makes it structurally impossible here is that `IconDto` never
 * carries a domain or a URL — only a bundled key naming a file in our own bundle, a
 * marker, or an integer. There is nothing to build a request from even by accident.
 *
 * ## The three cases
 *
 * | kind | drawn as | where the bytes are |
 * |---|---|---|
 * | `bundled` | `<img src="/icons/<key>.svg">` | in the app bundle, under `img-src 'self'` |
 * | `custom` | `<img src="data:…">` | encrypted in the item, fetched by id |
 * | `shape` | inline SVG | generated from a seed |
 *
 * Both image cases are an `<img>` and never `dangerouslySetInnerHTML`: an SVG inside an
 * `<img>` cannot execute script, so the rendering path is safe independently of what the
 * sanitiser believes about the bytes.
 */

import { cn } from '../lib/cn';
import { FallbackMark } from './FallbackMark';
import type { IconDto } from '../ipc';

export interface IdentityTileProps {
  /** How to draw it, resolved in Rust. */
  icon: IconDto;
  /** Tile edge in px. Radius and type scale from it (see `theme/dynamic.css`). */
  size?: 24 | 28 | 32 | 56 | 64;
  /**
   * The item title, for the accessible name.
   *
   * The tile is decorative wherever a visible title sits beside it, which is every case
   * in this product, so it is `aria-hidden` throughout.
   */
  title: string;
  /**
   * The user's icon as a `data:` URI, when `icon.kind` is `custom`.
   *
   * Passed in rather than fetched here: a component that fetched per render would issue
   * one IPC call per row per scroll frame. The owner fetches once and caches.
   */
  customSrc?: string | undefined;
  /** Which theme variant to prefer, for brands that ship a light/dark pair. */
  theme?: 'light' | 'dark' | undefined;
  className?: string;
}

export function IdentityTile({
  icon,
  size = 32,
  title,
  customSrc,
  theme,
  className,
}: IdentityTileProps) {
  if (icon.kind === 'bundled') {
    // A themed brand ships `<key>-light.svg` and `<key>-dark.svg` alongside the colour
    // mark. Never a recoloured version of one file — ADD-001: *"Never recolour a tier-1
    // mark to fit a theme."*
    const key = icon.themed && theme !== undefined ? `${icon.key}-${theme}` : icon.key;
    // A mark whose every ink is too dark to read on the dark tile gets a light chip
    // behind it instead of being recoloured — ADD-001 forbids recolouring, and a
    // recoloured brand mark is the wrong mark. Only marks the build flagged, so the
    // ones that already read on dark are untouched. A themed pair is never flagged:
    // the brand has already answered this question themselves.
    const chip = icon.darkInk ? 'light' : undefined;
    return (
      <span
        className={cn('tile', className)}
        data-size={size}
        data-tone="0"
        data-chip={chip}
        aria-hidden="true"
      >
        <img
          className="tile__icon"
          src={`/icons/${key}.svg`}
          alt=""
          loading="lazy"
          decoding="async"
        />
      </span>
    );
  }

  if (icon.kind === 'custom') {
    // No bytes yet is a normal state, not an error, and it must not render an empty
    // square. It happens on first paint before the fetch lands, and — the case that
    // reported this — in the moment after the user removes an icon: Rust has already
    // dropped the bytes while this row still says `custom`, because the list has not
    // refetched yet. Falling back to the default mark means the tile shows what the
    // item is about to look like anyway.
    if (customSrc === undefined) {
      return (
        <span
          className={cn('tile', className)}
          data-size={size}
          data-tone="fallback"
          aria-hidden="true"
        >
          <FallbackMark />
        </span>
      );
    }
    return (
      <span className={cn('tile', className)} data-size={size} data-tone="0" aria-hidden="true">
        <img className="tile__icon" src={customSrc} alt="" decoding="async" />
      </span>
    );
  }

  return (
    <span
      className={cn('tile', className)}
      data-size={size}
      data-tone="fallback"
      aria-hidden="true"
      title={title}
    >
      <FallbackMark />
    </span>
  );
}
