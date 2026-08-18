/**
 * The mark an item wears when no brand icon matches it.
 *
 * ## Why not initials
 *
 * Two letters on a coloured square is what this used to be, and it is the wrong answer
 * in a product where most tiles are real brand marks: a monogram next to a row of logos
 * reads as an image that failed to load. It also says nothing — the item's name is on the
 * row beside it, at a size you can actually read.
 *
 * A geometric mark is a *mark*. It is recognisable at 24px, it never collides with a
 * real logo, and it gives every item its own identity without pretending to be a brand.
 *
 * ## Deterministic, forever
 *
 * Everything is derived from the seed Rust computed from the item's registrable domain
 * (FNV-1a, a stable hash chosen precisely so it does not move between toolchains). The
 * same domain draws the same mark on every device the vault reaches and after every
 * update — which is what lets someone learn their vault by shape.
 *
 * Rust sends a seed rather than a shape name on purpose. *Which* shapes exist is a
 * design decision and belongs here with the token layer; what Rust owes is determinism.
 *
 * ## Colour
 *
 * The tile's fill is one of the seven `--identity-N` tones, applied by the `.tile` rules
 * in `theme/dynamic.css`. Every shape here is drawn in `--identity-text` at varying
 * opacity, so contrast is whatever the ramp already guarantees — the contrast report
 * puts all seven at ≥5.6:1 against white — and no new colour is introduced.
 */

import { cn } from '../lib/cn';

/** The drawing grid. Every path below is authored against this box. */
const BOX = 32;

/**
 * The shape vocabulary.
 *
 * Eight families, each a composition of primitives rather than a glyph. They are drawn
 * to read at 24px: no stroke under 2 units, no detail smaller than 3, nothing that
 * depends on a corner staying sharp.
 */
const FAMILIES = 8;

/** Renders one family. Opacity varies so overlapping parts read as depth. */
function shapeFor(family: number, variant: number): React.ReactNode {
  const strong = 0.95;
  const soft = variant === 0 ? 0.4 : variant === 1 ? 0.55 : 0.3;

  switch (family) {
    case 0:
      // Disc with an offset companion.
      return (
        <>
          <circle cx="13" cy="13" r="8" fillOpacity={soft} />
          <circle cx="20" cy="20" r="6" fillOpacity={strong} />
        </>
      );
    case 1:
      // Rounded square, inset.
      return (
        <>
          <rect x="7" y="7" width="18" height="18" rx="5" fillOpacity={soft} />
          <rect x="13" y="13" width="12" height="12" rx="3.5" fillOpacity={strong} />
        </>
      );
    case 2:
      // Diagonal split.
      return (
        <>
          <path d="M4 28 L28 4 L28 28 Z" fillOpacity={strong} />
          <path d="M4 28 L28 4 L4 4 Z" fillOpacity={soft} />
        </>
      );
    case 3:
      // Quarter round, the shape of a bracket.
      return (
        <>
          <path d="M6 26 V14 A12 12 0 0 1 18 2 h8 v10 a12 12 0 0 0-12 12 Z" fillOpacity={strong} />
          <circle cx="23" cy="23" r="4.5" fillOpacity={soft} />
        </>
      );
    case 4:
      // Concentric ring and core.
      return (
        <>
          <path
            d="M16 4a12 12 0 1 0 0 24 12 12 0 1 0 0-24Zm0 5a7 7 0 1 1 0 14 7 7 0 0 1 0-14Z"
            fillOpacity={soft}
          />
          <circle cx="16" cy="16" r="4" fillOpacity={strong} />
        </>
      );
    case 5:
      // Three bars, stepped.
      return (
        <>
          <rect x="5" y="7" width="22" height="5" rx="2.5" fillOpacity={soft} />
          <rect x="5" y="14" width="15" height="5" rx="2.5" fillOpacity={strong} />
          <rect x="5" y="21" width="19" height="5" rx="2.5" fillOpacity={soft} />
        </>
      );
    case 6:
      // Diamond over a square.
      return (
        <>
          <rect x="8" y="8" width="16" height="16" rx="3" fillOpacity={soft} />
          <path d="M16 5 L27 16 L16 27 L5 16 Z" fillOpacity={strong} />
        </>
      );
    default:
      // Two rounded bars crossing.
      return (
        <>
          <rect x="4" y="12.5" width="24" height="7" rx="3.5" fillOpacity={soft} />
          <rect x="12.5" y="4" width="7" height="24" rx="3.5" fillOpacity={strong} />
        </>
      );
  }
}

export interface GeneratedMarkProps {
  /** The seed Rust derived from the item's identity. */
  seed: number;
  className?: string;
}

/**
 * A deterministic geometric mark.
 *
 * Drawn as inline SVG rather than an `<img>`: it needs `currentColor` from the tile, and
 * there is no file to fetch. Presentation attributes, not the `style` prop — the
 * production CSP drops a markup `style` attribute (SPEC-V1 §7.6) and `fill-opacity` is
 * an SVG attribute, so this needs no exception.
 */
export function GeneratedMark({ seed, className }: GeneratedMarkProps) {
  // Three independent decisions, taken from three parts of the seed so they do not move
  // together. `>>> 0` keeps the arithmetic unsigned; Rust sends a u32 and JavaScript
  // bitwise operators are signed.
  const s = seed >>> 0;
  const family = s % FAMILIES;
  const rotation = ((s >>> 3) % 4) * 90;
  const variant = (s >>> 5) % 3;

  return (
    <svg
      className={cn('tile__mark', className)}
      viewBox={`0 0 ${String(BOX)} ${String(BOX)}`}
      fill="currentColor"
      aria-hidden="true"
      focusable="false"
    >
      {/* Rotation about the centre, so a family reads as four related marks rather
          than four unrelated ones. */}
      <g transform={`rotate(${String(rotation)} ${String(BOX / 2)} ${String(BOX / 2)})`}>
        {shapeFor(family, variant)}
      </g>
    </svg>
  );
}
