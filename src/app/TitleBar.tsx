/**
 * Title bar — components.md §2.
 *
 * Closes three of the accessibility gaps the handoff lists: the search trigger, the
 * theme toggle and Lock are real buttons rather than divs with onClick, so they are
 * reachable by keyboard and carry the focus ring from `a11y.css`.
 *
 * Traffic lights render on macOS only. Launching the real window showed why: the
 * design is macOS-shaped and draws its own, so on Windows the app had the native title
 * bar AND a decorative row of fake controls below it. SPEC-V1 §8's platform table
 * settles it — "native traffic lights" on macOS, "native controls" on Windows — so
 * this is implementing the spec rather than choosing a composition. Their *position*
 * within the bar is still the design's and is raised for HO-002.
 */

import { useNavigation } from './navigation';
import { Glyph } from '../components/Glyph';
import type { GlyphName } from '../components/Glyph';
import { useThemeStore } from '../theme/store';
import type { ThemeMode } from '../theme/mode';

export interface TitleBarProps {
  /** Opens the command palette. */
  onOpenPalette: () => void;
  /** Locks the vault. */
  onLock: () => void;
  /** The platform's modifier label, from `app_platform_info`. Never hardcoded. */
  modifierKey: string;
  /** Host OS, from `app_platform_info`. Decides which window chrome renders. */
  os: string;
}

/** Cycle order for the toggle: what the user gets on the next press. */
const NEXT: Record<ThemeMode, ThemeMode> = { dark: 'light', light: 'system', system: 'dark' };

/** Glyph per mode, matching the design's sun/moon pairing plus one for `system`. */
const MODE_GLYPH: Record<ThemeMode, GlyphName> = {
  dark: 'themeDark',
  light: 'themeLight',
  system: 'themeSystem',
};

/**
 * The design's label is binary — "Light" / "Dark". Ours is tri-state because
 * CLAUDE.md §3 requires `system` following the OS, so the label names the stored
 * preference rather than the resolved palette. A toggle reading "Dark" while set to
 * `system` on a dark OS would be indistinguishable from an explicit choice.
 */
const MODE_LABEL: Record<ThemeMode, string> = {
  dark: 'Dark',
  light: 'Light',
  system: 'System',
};

export function TitleBar({ onOpenPalette, onLock, modifierKey, os }: TitleBarProps) {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const search = useNavigation((s) => s.search);

  return (
    <header className="titlebar" data-tauri-drag-region>
      {/* SPEC-V1 §8's platform table: "Window chrome | native traffic lights |
          native controls". The design is macOS-shaped and draws its own traffic
          lights; on Windows the OS already draws real controls, so rendering
          decorative ones below them gives the window two sets of chrome — which is
          what launching it actually showed. Rendering them only on macOS is
          implementing §8, not redesigning. */}
      {os === 'macos' ? (
        <div className="titlebar__lights" aria-hidden="true">
          <span className="light light--close" />
          <span className="light light--minimise" />
          <span className="light light--zoom" />
        </div>
      ) : null}

      <div className="titlebar__brand">
        <span className="wordmark__mark" aria-hidden="true" />
        <span className="wordmark__text">Keyring</span>
      </div>

      {/* components.md §2 names this a gap: not implemented, it is a div, give it a
          tabindex and the ring. It is a button instead. */}
      <button type="button" className="search-trigger" onClick={onOpenPalette}>
        <Glyph name="search" />
        <span className="search-trigger__label">
          {search === '' ? 'Search vault, actions, tags' : search}
        </span>
        <kbd className="kbd">{modifierKey}K</kbd>
      </button>

      <div className="titlebar__actions">
        <button
          type="button"
          className="toolbar-action"
          onClick={() => void setMode(NEXT[mode])}
          // Names the current state and the next one: a three-state toggle is
          // unguessable from an icon alone.
          aria-label={`Appearance: ${MODE_LABEL[mode]}. Switch to ${MODE_LABEL[NEXT[mode]]}`}
        >
          <Glyph name={MODE_GLYPH[mode]} />
          {MODE_LABEL[mode]}
        </button>
        <button type="button" className="toolbar-action" onClick={onLock}>
          <Glyph name="lock" />
          Lock
        </button>
      </div>
    </header>
  );
}
