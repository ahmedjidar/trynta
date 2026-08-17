/**
 * Title bar — components.md §2.
 *
 * Closes three of the accessibility gaps the handoff lists: the search trigger, the
 * theme toggle and Lock are real buttons rather than divs with onClick, so they are
 * reachable by keyboard and carry the focus ring from `a11y.css`.
 *
 * Traffic lights are macOS-shaped in the design and stay where HO-001 puts them on
 * both platforms. Mirroring them to the right on Windows would be a second
 * composition the handoff does not specify, and that is designing. Raised for HO-002.
 */

import { useNavigation } from './navigation';
import { useThemeStore } from '../theme/store';
import type { ThemeMode } from '../theme/mode';

export interface TitleBarProps {
  /** Opens the command palette. */
  onOpenPalette: () => void;
  /** Locks the vault. */
  onLock: () => void;
  /** The platform's modifier label, from `app_platform_info`. Never hardcoded. */
  modifierKey: string;
}

/** Cycle order for the toggle: what the user gets on the next press. */
const NEXT: Record<ThemeMode, ThemeMode> = { dark: 'light', light: 'system', system: 'dark' };

export function TitleBar({ onOpenPalette, onLock, modifierKey }: TitleBarProps) {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const search = useNavigation((s) => s.search);

  return (
    <header className="titlebar" data-tauri-drag-region>
      <div className="titlebar__lights" aria-hidden="true">
        <span className="light light--close" />
        <span className="light light--minimise" />
        <span className="light light--zoom" />
      </div>

      <div className="titlebar__brand">
        <span className="wordmark__mark" aria-hidden="true" />
        <span className="wordmark__text">Keyring</span>
      </div>

      {/* components.md §2 names this a gap: not implemented, it is a div, give it a
          tabindex and the ring. It is a button instead. */}
      <button type="button" className="search-trigger" onClick={onOpenPalette}>
        <span className="search-trigger__label">{search === '' ? 'Search' : search}</span>
        <kbd className="kbd">{modifierKey}K</kbd>
      </button>

      <div className="titlebar__actions">
        <button
          type="button"
          className="toolbar-action"
          onClick={() => void setMode(NEXT[mode])}
          // Names the current state and the next one: a three-state toggle is
          // unguessable from an icon alone.
          aria-label={`Theme: ${mode}. Switch to ${NEXT[mode]}`}
        >
          {mode}
        </button>
        <button type="button" className="toolbar-action" onClick={onLock}>
          Lock
        </button>
      </div>
    </header>
  );
}
