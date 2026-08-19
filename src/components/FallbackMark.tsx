/**
 * The mark an item gets when no brand icon and no custom icon apply.
 *
 * **The single point of change for the fallback.** `IdentityTile` renders this and
 * nothing else for tier 3, so replacing the fallback is replacing this file — no
 * other component knows what an unmapped item looks like.
 *
 * ## Why a lock and not a generated shape
 *
 * It was eight shape families seeded from the registrable domain, on the reasoning
 * that a stable arbitrary mark gives an unmapped item an identity you can learn.
 * That reasoning is sound and the result was not: next to a row of real logos, a
 * procedural glyph reads as decoration at best and as a rendering fault at worst.
 * Deliberate and plain beats clever and odd, so every unmapped item now gets the
 * same considered lock.
 *
 * The consequence is accepted rather than hidden: unmapped items no longer differ
 * from one another. Their titles differ, they sit in a list sorted by something the
 * user chose, and none of that depended on the tile.
 *
 * ## Why it is drawn here rather than taken from the glyph set
 *
 * The interface glyphs are stroked line icons sized for 12–16px next to text. This
 * is a *mark*: it fills a 24–64px tile, so it is drawn solid, with the shackle and
 * body as one path so no hairline breaks up at small sizes, and with the keyhole
 * punched out rather than painted — that way it reads on any tile colour, which is
 * what "suitable for all themes" has to mean.
 *
 * Colour comes from `currentColor` and the tile's own background, both tokens, so
 * this file introduces no colour of its own and needs no light/dark variant.
 */

/**
 * A solid padlock, sized to fill its tile.
 *
 * Takes nothing. Rust still resolves a stable per-identity seed and `IconDto` still
 * carries it — that is deliberate and costs nothing: giving the fallback per-item
 * variation again becomes a change to this file and its one call site, rather than
 * one that reaches back through the DTO into the store.
 */
export function FallbackMark() {
  return (
    <svg
      className="fallback-mark"
      viewBox="0 0 32 32"
      role="presentation"
      aria-hidden="true"
      focusable="false"
    >
      {/* The shackle: a stroked arc, so its weight tracks the body's optical mass at
          every tile size rather than being a second filled shape to keep in step. */}
      <path
        d="M11 14v-3.2a5 5 0 0 1 10 0V14"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
      {/* The body, with the keyhole punched out by the even-odd rule rather than
          painted in a second colour. A painted keyhole has to match the tile behind
          it; a hole is correct on every background there will ever be. */}
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M8.4 14h15.2c.77 0 1.4.63 1.4 1.4v9.2c0 .77-.63 1.4-1.4 1.4H8.4A1.4 1.4 0 0 1 7 24.6v-9.2c0-.77.63-1.4 1.4-1.4Zm7.6 4a1.9 1.9 0 0 0-.8 3.62v1.63a.8.8 0 0 0 1.6 0v-1.63A1.9 1.9 0 0 0 16 18Z"
        fill="currentColor"
      />
    </svg>
  );
}
