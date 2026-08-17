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
  Check,
  ChevronRight,
  ChevronsUpDown,
  CreditCard,
  Fingerprint,
  KeyRound,
  Lock,
  Monitor,
  Moon,
  Plus,
  Search,
  Settings,
  Shield,
  ShieldCheck,
  Sparkles,
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
  generate: Sparkles,
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
} as const;

/** A glyph role. */
export type GlyphName = keyof typeof GLYPHS;

export interface GlyphProps {
  /** Which glyph. */
  name: GlyphName;
  /**
   * Accessible name.
   *
   * Omit for a glyph that sits beside its own visible label, which is most of them —
   * announcing "lock, Lock" is worse than announcing nothing.
   */
  label?: string;
}

export function Glyph({ name, label }: GlyphProps) {
  const Component = GLYPHS[name];
  return (
    <Component
      className="glyph"
      // The token values, passed as attributes because SVG presentation attributes
      // are not the CSS `style` prop and are not affected by `style-src`. Size and
      // stroke still come from the design: `--icon-size` 16px, `--icon-stroke` 1.75.
      size={16}
      strokeWidth={1.75}
      aria-hidden={label === undefined ? true : undefined}
      aria-label={label}
      role={label === undefined ? undefined : 'img'}
      focusable={false}
    />
  );
}
