/**
 * Application shell.
 *
 * UNSTYLED: awaiting the shell composition — HO-001 covers it (components.md §1-4)
 * and it lands next. What exists now is the theme layer, wired end to end: the
 * stored mode is read from `app_state` before first paint, `system` follows the OS
 * live, and an imported theme is applied through the constructible-stylesheet loader
 * under the production CSP.
 *
 * The markup below is deliberately structural and ugly. It is a harness for
 * exercising the theme, not a design — placeholder styling never gets replaced, it
 * gets shipped (CLAUDE.md §3).
 */

import { useEffect } from 'react';

import { useThemeStore } from '../theme/store';
import type { ThemeMode } from '../theme/mode';

const MODES: readonly ThemeMode[] = ['dark', 'light', 'system'];

export function App() {
  const mode = useThemeStore((s) => s.mode);
  const resolved = useThemeStore((s) => s.resolved);
  const ready = useThemeStore((s) => s.ready);
  const hydrate = useThemeStore((s) => s.hydrate);
  const setMode = useThemeStore((s) => s.setMode);

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  return (
    <main>
      <h1>Keyring</h1>
      <p>Pre-1.0. Not ready for real credentials.</p>

      {/* UNSTYLED: awaiting handoff app-shell. The real control is the title bar's
          theme toggle (components.md §2). */}
      <fieldset>
        <legend>Theme</legend>
        {MODES.map((option) => (
          <label key={option}>
            <input
              type="radio"
              name="theme-mode"
              value={option}
              checked={mode === option}
              onChange={() => void setMode(option)}
            />
            {option}
          </label>
        ))}
        <p>
          {/* data-testid, not a class: this is a harness affordance and will not
              survive into the styled shell. */}
          <span data-testid="resolved-theme">{resolved}</span>
          {ready ? '' : ' (reading preference…)'}
        </p>
      </fieldset>
    </main>
  );
}
