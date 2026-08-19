// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Theme state (SPEC-V1 §7.6).
 *
 * Owns three things and keeps them in agreement:
 *
 * 1. the stored **mode** — `dark` | `light` | `system`;
 * 2. the **resolved** theme, which is what is actually on screen;
 * 3. the active **imported theme**, applied through the constructible-stylesheet
 *    loader.
 *
 * ## Why the DOM is written here and not in a component
 *
 * `data-theme` goes on `<html>`, which is outside React's tree. A `useEffect` in a
 * provider would work, and would also mean the attribute is applied one paint after
 * the first render — a visible flash of the wrong theme on every launch, and on
 * every mode change in StrictMode's double-invoke. The store writes the attribute
 * synchronously in the same action that changes the state, so the two can never be a
 * frame apart.
 *
 * ## Failure is not an error
 *
 * Every path here degrades to the built-in palette rather than throwing. A theme is
 * cosmetic; a vault is not. If `theme_list` fails because no vault exists yet, or an
 * imported theme cannot be applied because the webview has no constructible
 * stylesheets, the app renders in the designed default and says nothing. The one
 * thing it must never do is leave the UI unstyled because a preference could not be
 * read.
 */

import { create } from 'zustand';

import type { ThemeDto } from '../ipc';
import { themeList, themeSet } from '../ipc';
import * as loader from './loader';
import type { ResolvedTheme, ThemeMode } from './mode';
import { applyTheme, resolveTheme, watchSystemTheme } from './mode';

/** Theme state and the actions that change it. */
export interface ThemeState {
  /** The stored preference. */
  mode: ThemeMode;
  /** What is actually rendering. Equal to `mode` unless `mode` is `system`. */
  resolved: ResolvedTheme;
  /** Imported themes available to pick. Empty while the vault is locked. */
  imported: readonly ThemeDto[];
  /** The active imported theme's id, or `null` for the built-in palette. */
  activeId: string | null;
  /** Whether imported themes are unavailable because the vault is locked. */
  locked: boolean;
  /** Whether the initial read has completed. */
  ready: boolean;

  /** Read the stored selection and apply it. Safe to call more than once. */
  hydrate: () => Promise<void>;
  /** Change the mode, persisting it. */
  setMode: (mode: ThemeMode) => Promise<void>;
  /** Activate an imported theme, or `null` for the built-in palette. */
  setTheme: (id: string | null) => Promise<void>;
  /**
   * Drop the imported themes and go back to the built-in palette.
   *
   * Called on lock. The values came out of the encrypted settings blob, and §4.9's
   * "lock is real" does not have an exception for the ones that happen to be
   * colours: after a lock the webview should hold no more than the lock screen
   * needs, which is the mode and nothing else.
   */
  forget: () => void;
  /** Re-read after unlock, when imported themes become available. */
  refresh: () => Promise<void>;
}

/** Disposer for the OS-preference listener, if one is attached. */
let stopWatching: (() => void) | null = null;

/**
 * Attach or detach the `prefers-color-scheme` listener.
 *
 * Only `system` mode needs it. Leaving it attached in `dark` would mean an OS change
 * repainting a user who explicitly asked not to follow the OS.
 */
function watch(mode: ThemeMode, onSystemChange: (theme: ResolvedTheme) => void): void {
  stopWatching?.();
  stopWatching = null;
  if (mode === 'system') {
    stopWatching = watchSystemTheme(onSystemChange);
  }
}

/**
 * Apply the imported theme matching `id`, or clear it.
 *
 * A theme whose `mode` does not match the resolved theme is still applied — the
 * loader scopes it to the right selector, so a light theme sitting inactive while
 * dark renders is correct and costs nothing.
 */
function applyImported(imported: readonly ThemeDto[], id: string | null): void {
  const theme = id === null ? undefined : imported.find((t) => t.id === id);
  if (!theme) {
    loader.clear();
    return;
  }
  loader.apply({
    id: theme.id,
    name: theme.name,
    mode: theme.mode,
    tokens: theme.tokens,
  });
}

export const useThemeStore = create<ThemeState>((set, get) => ({
  // The token layer's base is dark, so that is the pre-hydration value: it is what
  // renders if the read never completes, and matching it avoids a flash.
  mode: 'system',
  resolved: 'dark',
  imported: [],
  activeId: null,
  locked: true,
  ready: false,

  hydrate: async () => {
    // Apply the resolved default immediately. Waiting for IPC before touching the
    // DOM is what produces a flash of the wrong theme on launch.
    const fallback = resolveTheme(get().mode);
    applyTheme(fallback);

    let catalogue;
    try {
      catalogue = await themeList();
    } catch {
      // No vault yet, or app_state unreadable. The designed default is the right
      // answer and there is nothing useful to tell the user.
      set({ resolved: fallback, ready: true });
      watch(get().mode, (theme) => {
        applyTheme(theme);
        set({ resolved: theme });
      });
      return;
    }

    const mode = catalogue.mode;
    const resolved = resolveTheme(mode);
    applyTheme(resolved);
    // Before the first `applyImported`, which is the only moment the built-in values
    // are certainly the ones on the root. The settings list draws a "Built-in" swatch
    // from this; read live it would show whichever theme is active.
    loader.noteBuiltIn();
    applyImported(catalogue.imported, catalogue.activeId);

    set({
      mode,
      resolved,
      imported: catalogue.imported,
      activeId: catalogue.activeId,
      locked: catalogue.locked,
      ready: true,
    });

    watch(mode, (theme) => {
      applyTheme(theme);
      set({ resolved: theme });
    });
  },

  setMode: async (mode) => {
    const resolved = resolveTheme(mode);
    // Optimistic and synchronous: the user clicked a toggle and expects the window
    // to change now, not after a round trip. A failed write means the preference
    // does not survive a restart, which is worth far less than the latency.
    applyTheme(resolved);
    set({ mode, resolved });
    watch(mode, (theme) => {
      applyTheme(theme);
      set({ resolved: theme });
    });

    try {
      await themeSet(get().activeId, mode);
    } catch {
      // Deliberately silent. See the module note.
    }
  },

  setTheme: async (id) => {
    // Captured before the optimistic set, or it reads back the id we just wrote and
    // "restore the previous selection" restores the one that failed.
    const previous = get().activeId;
    applyImported(get().imported, id);
    set({ activeId: id });
    try {
      await themeSet(id, get().mode);
    } catch {
      // The write failed, so the stored selection still names the old theme. Put
      // the applied CSS *and* the store back in agreement with it rather than
      // leaving them disagreeing — the opposite of `setMode`, because here the
      // mismatch would persist visibly instead of resetting on the next launch.
      applyImported(get().imported, previous);
      set({ activeId: previous });
    }
  },

  forget: () => {
    applyImported([], null);
    set({ imported: [], activeId: null });
  },

  refresh: async () => {
    try {
      const catalogue = await themeList();
      const resolved = resolveTheme(catalogue.mode);
      applyTheme(resolved);
      applyImported(catalogue.imported, catalogue.activeId);
      set({
        mode: catalogue.mode,
        resolved,
        imported: catalogue.imported,
        activeId: catalogue.activeId,
        locked: catalogue.locked,
      });
      watch(catalogue.mode, (theme) => {
        applyTheme(theme);
        set({ resolved: theme });
      });
    } catch {
      // Leave the current theme in place.
    }
  },
}));
