/**
 * The identity tile — bundled brand icon or monogram (ADD-001, components.md §4).
 *
 * The prototype fetched a favicon here. This cannot: it is handed an
 * {@link IconDto}, which is either a key naming a file inside our own bundle or a
 * pair of initials, and there is no field on it a URL could be built from. Rust
 * resolved the domain; the webview never sees one.
 *
 * A bundled icon renders through `<img src>` rather than inlined SVG, per ADD-001:
 * an SVG in an `<img>` cannot execute script, which also means no
 * `dangerouslySetInnerHTML`.
 */

import type { IconDto } from '../ipc';

/** Tile sizes the design uses (`--size-tile`, `-lg`, `-xl`). */
export type TileSize = 'sm' | 'md' | 'lg' | 'xl';

export interface IdentityTileProps {
  /** How to draw it, resolved in Rust. */
  icon: IconDto;
  /** Which size step. */
  size?: TileSize;
  /**
   * The item title, for the accessible name.
   *
   * The tile is decorative when it sits next to a visible title — which is every
   * case in HO-001 — so it is `aria-hidden` and this is used only to build a
   * meaningful `alt` when the icon is the sole content.
   */
  title: string;
  /** Whether the tile carries the accessible name itself. */
  labelled?: boolean;
}

export function IdentityTile({ icon, size = 'md', title, labelled = false }: IdentityTileProps) {
  const className = `tile tile--${size}`;

  if (icon.kind === 'bundled') {
    return (
      <span className={className} data-bundled>
        {/* Relative, same-origin, and built from a key Rust chose from a closed
            set — not from anything the item contains. */}
        <img
          className="tile__icon"
          src={`icons/${icon.key}.svg`}
          alt={labelled ? title : ''}
          aria-hidden={labelled ? undefined : true}
          width={20}
          height={20}
          draggable={false}
        />
      </span>
    );
  }

  return (
    <span
      className={className}
      data-tone={icon.tone}
      role={labelled ? 'img' : undefined}
      aria-label={labelled ? title : undefined}
      aria-hidden={labelled ? undefined : true}
    >
      <span className="tile__monogram">{icon.initials}</span>
    </span>
  );
}
