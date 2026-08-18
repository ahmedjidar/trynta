/**
 * Identity tile — HO-002 `ui/IdentityTile.tsx`, with the favicon layer removed.
 *
 * ## What changed, and why it is not negotiable
 *
 * HO-002 renders a monogram tile and lays a Google favicon service request over it
 * (`https://www.google.com/s2/favicons?domain=…`). That is one request per item to a third
 * party, keyed by the domain the item is for — which is an inventory of the vault, sent
 * out, item by item. ADD-001 forbids it, SPEC-V1 §11's packet-capture criterion tests for
 * it, `check:network` fails the build on it, and the production CSP's `img-src 'self'
 * data:` would block it anyway.
 *
 * So the tile is the monogram, and where a brand icon exists it comes from the bundled set
 * that Rust already resolved. `IconDto` is that decision, made in one place: `Bundled` when
 * the registrable domain matched a bundled key, `Monogram` otherwise, with the tone Rust
 * chose so a re-render cannot change an item's colour.
 *
 * HO-002 keeps the monogram *underneath* the favicon so a blocked request degrades without
 * layout shift. Nothing can be blocked here, so there is nothing to degrade to — the two
 * cases are exclusive.
 */

import { cn } from '../lib/cn';
import type { IconDto } from '../ipc';

export interface IdentityTileProps {
  /** How to draw it, resolved in Rust. */
  icon: IconDto;
  /** Tile edge in px. Radius and type scale from it (see `dynamic.css`). */
  size?: 24 | 28 | 32 | 56 | 64;
  /**
   * The item title, for the accessible name.
   *
   * The tile is decorative wherever a visible title sits beside it — which is every case
   * in HO-002 — so it is `aria-hidden` and this only builds the `alt` when a bundled icon
   * is the sole content.
   */
  title: string;
  className?: string;
}

export function IdentityTile({ icon, size = 32, title, className }: IdentityTileProps) {
  if (icon.kind === 'bundled') {
    return (
      <span className={cn('tile', className)} data-size={size} data-tone="0" aria-hidden="true">
        {/* Bundled asset under `img-src 'self'`. Never a remote URL, and an <img> cannot
            execute script even if the file were replaced. */}
        <img className="tile__icon" src={`/icons/${icon.key}.svg`} alt="" />
      </span>
    );
  }

  return (
    <span
      className={cn('tile', className)}
      data-size={size}
      data-tone={icon.tone}
      aria-hidden="true"
      title={title}
    >
      {icon.initials}
    </span>
  );
}
