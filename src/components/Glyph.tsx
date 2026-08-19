/**
 * UI glyphs — bundled, never fetched.
 *
 * The design specifies Lucide icons and loads them from a CDN
 * (`unpkg.com/lucide@…`). That is a runtime network request, so it is out: CLAUDE.md
 * §4.7 permits exactly three and neither of the other two is an icon. `check:network`
 * fails the build on that host.
 *
 * `lucide-react` is the same icon set as an npm dependency, so the glyphs ship inside
 * the bundle and nothing is fetched. It is a pure-render dependency — no I/O, no
 * network, no serialisation, no access to a secret — which is why it does not need the
 * CLAUDE.md §2 conversation that a crate touching key material would.
 *
 * **Distinct from `IdentityTile`, and the distinction matters.** These are interface
 * glyphs: a search magnifier, a lock, a chevron. `IdentityTile` renders *brand* marks,
 * which ADD-001 governs, because a brand mark identifies a service the user holds an
 * account with and fetching one leaks the vault's contents. A chevron leaks nothing.
 *
 * The indirection exists so component code names a glyph by role rather than importing
 * from the icon library directly. One file to change if the set is ever swapped, and
 * one place where the size and stroke tokens are applied.
 */

import {
  Activity,
  AlertTriangle,
  Check,
  ChevronRight,
  ChevronsUpDown,
  CreditCard,
  Dices,
  Fingerprint,
  KeyRound,
  Lock,
  Maximize2,
  Minimize2,
  Minus,
  Monitor,
  Moon,
  Plus,
  Search,
  Settings,
  Shield,
  ShieldCheck,
  Star,
  StickyNote,
  Sun,
  User,
  X,
} from 'lucide-react';

/**
 * Every glyph the interface uses, by role.
 *
 * Named for what it means here rather than what the library calls it, so a component
 * asks for `sort` rather than `chevrons-up-down`.
 */
const GLYPHS = {
  search: Search,
  lock: Lock,
  themeDark: Moon,
  themeLight: Sun,
  themeSystem: Monitor,
  sort: ChevronsUpDown,
  add: Plus,
  next: ChevronRight,
  close: X,
  check: Check,
  // Dice, not a sparkle. A sparkle has become the house mark for "a language model
  // did this", and nothing here involves one: the generator is a CSPRNG with
  // rejection sampling (§7.3). Dice say *random*, which is what the button does.
  generate: Dices,
  security: Activity,
  settings: Settings,
  biometric: Fingerprint,
  verified: ShieldCheck,
  all: Shield,
  login: KeyRound,
  note: StickyNote,
  card: CreditCard,
  identity: User,
  favorite: Star,
  warning: AlertTriangle,
  windowMinimise: Minus,
  windowMaximise: Maximize2,
  windowRestore: Minimize2,
  windowClose: X,
} as const;

/** A glyph role. */
export type GlyphName = keyof typeof GLYPHS;

export interface GlyphProps {
  /** Which glyph. */
  name: GlyphName;
  /**
   * Edge in px. The design sizes icons per use — 12 beside chip text, 14 in a segment or
   * a "Fix" affordance, 16 everywhere else — so the default is the token and the two
   * smaller steps are the design's own.
   */
  size?: 12 | 14 | 16;
  /**
   * Accessible name.
   *
   * Omit for a glyph that sits beside its own visible label, which is most of them —
   * announcing "lock, Lock" is worse than announcing nothing.
   */
  label?: string;
}

export function Glyph({ name, size = 16, label }: GlyphProps) {
  const Component = GLYPHS[name];
  return (
    <Component
      // `lucide` so base.css's `svg.lucide` rule applies the token size and stroke, which
      // is the design's own mechanism. The attributes below carry the same values for the
      // case where a rule has not matched yet.
      className="lucide"
      // Passed as attributes because SVG presentation attributes are not the CSS `style`
      // prop and so are not affected by `style-src`. Size and stroke come from the
      // design: `--icon-size` 16px, `--icon-stroke` 1.75.
      size={size}
      strokeWidth={1.75}
      aria-hidden={label === undefined ? true : undefined}
      aria-label={label}
      role={label === undefined ? undefined : 'img'}
      focusable={false}
    />
  );
}
