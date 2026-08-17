/**
 * Theme layer unit tests (SPEC-V1 §7.6, AC17, AC18).
 *
 * Two properties matter here and neither is about appearance:
 *
 * - **`system` resolves to something, always.** A mode that resolves to `undefined`
 *   renders an unstyled app, and the failure only shows up on a machine whose
 *   `matchMedia` behaves differently from the developer's.
 * - **The applied CSS is scoped to the palette it replaces.** A light theme that
 *   leaks into dark is worse than no theme: the tokens it does not define fall
 *   through to the dark values and the result is unreadable in a way no contrast
 *   report predicts.
 *
 * The CSP half of AC18 — that `adoptedStyleSheets` works where an injected `<style>`
 * would be blocked — cannot be asserted here. happy-dom enforces no CSP, so a test
 * that passed would prove nothing. It needs the E2E harness against a real WebView2,
 * which is the frozen AC18 row.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { cssFor, type LoadedTheme } from './loader';
import { applyTheme, isThemeMode, resolveTheme, systemTheme, watchSystemTheme } from './mode';

/** Install a `matchMedia` that reports the given preference. */
function stubMatchMedia(prefersLight: boolean, listeners: Array<(e: MediaQueryListEvent) => void>) {
  vi.stubGlobal(
    'matchMedia',
    (query: string) =>
      ({
        matches: query.includes('light') ? prefersLight : !prefersLight,
        media: query,
        addEventListener: (_: string, handler: (e: MediaQueryListEvent) => void) => {
          listeners.push(handler);
        },
        removeEventListener: (_: string, handler: (e: MediaQueryListEvent) => void) => {
          const at = listeners.indexOf(handler);
          if (at >= 0) listeners.splice(at, 1);
        },
      }) as unknown as MediaQueryList,
  );
}

describe('mode resolution', () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    document.documentElement.removeAttribute('data-theme');
  });

  it('accepts exactly the three modes', () => {
    expect(isThemeMode('dark')).toBe(true);
    expect(isThemeMode('light')).toBe(true);
    expect(isThemeMode('system')).toBe(true);

    // Reading app_state, which is plaintext and hand-editable.
    for (const rubbish of ['', 'Dark', 'auto', 'system ', null, undefined, 0, {}]) {
      expect(isThemeMode(rubbish)).toBe(false);
    }
  });

  it('resolves an explicit mode without consulting the OS', () => {
    stubMatchMedia(true, []);
    expect(resolveTheme('dark')).toBe('dark');
    expect(resolveTheme('light')).toBe('light');
  });

  it('resolves system from the OS preference', () => {
    stubMatchMedia(true, []);
    expect(systemTheme()).toBe('light');
    expect(resolveTheme('system')).toBe('light');

    stubMatchMedia(false, []);
    expect(systemTheme()).toBe('dark');
    expect(resolveTheme('system')).toBe('dark');
  });

  it('falls back to dark when matchMedia is missing', () => {
    // Not a hypothetical: a webview without it would otherwise resolve to
    // `undefined` and render the app with no palette at all. Dark is the token
    // layer's base, so it is the value that needs no override to be correct.
    vi.stubGlobal('matchMedia', undefined);
    expect(systemTheme()).toBe('dark');
    expect(resolveTheme('system')).toBe('dark');
  });

  it('sets the attribute for light and clears it for dark', () => {
    // Dark must CLEAR rather than set data-theme="dark": the dark values live on
    // bare `:root`, and no selector matches `[data-theme="dark"]`. Setting it would
    // work by accident until someone adds that block.
    applyTheme('light');
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    applyTheme('dark');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });

  it('stops following the OS once unsubscribed', () => {
    const listeners: Array<(e: MediaQueryListEvent) => void> = [];
    stubMatchMedia(false, listeners);

    const seen: string[] = [];
    const stop = watchSystemTheme((theme) => {
      seen.push(theme);
    });
    expect(listeners).toHaveLength(1);

    listeners[0]?.({ matches: true } as MediaQueryListEvent);
    expect(seen).toEqual(['light']);

    stop();
    expect(listeners).toHaveLength(0);
  });

  it('is inert when matchMedia is missing rather than throwing', () => {
    vi.stubGlobal('matchMedia', undefined);
    const stop = watchSystemTheme(() => {
      throw new Error('must not be called');
    });
    expect(() => {
      stop();
    }).not.toThrow();
  });
});

describe('theme serialisation', () => {
  // Lengths, not colours, and deliberately: `cssFor` is a string serialiser that
  // never parses a value, so a colour fixture would test nothing extra and would be
  // a hardcoded colour in the repo — which `pnpm check:tokens` fails, correctly. The
  // Rust validator is what decides whether a value is admissible.
  const tokens = { '--accent': 'var(--accent-hover)', '--space-1': '4px' };

  it('scopes a light theme so it cannot leak into dark', () => {
    const theme: LoadedTheme = { id: 't', name: 'T', mode: 'light', tokens };
    const css = cssFor(theme);
    expect(css).toContain(':root[data-theme="light"]');
    expect(css).not.toContain(':root:not([data-theme])');
  });

  it('scopes a dark theme so it cannot leak into light', () => {
    const theme: LoadedTheme = { id: 't', name: 'T', mode: 'dark', tokens };
    const css = cssFor(theme);
    // `:root:not([data-theme])` and not bare `:root`, because bare `:root` would
    // also match while light is active and the light overrides would only
    // partially win depending on declaration order.
    expect(css).toContain(':root:not([data-theme])');
    expect(css).not.toContain('[data-theme="light"]');
  });

  it('emits every token as a declaration', () => {
    const css = cssFor({ id: 't', name: 'T', mode: 'dark', tokens });
    expect(css).toContain('--accent: var(--accent-hover);');
    expect(css).toContain('--space-1: 4px;');
  });

  it('is byte-identical for the same theme twice', () => {
    // Rust returns `tokens` sorted for this reason: it makes re-applying a theme a
    // no-op the loader can compare cheaply, and makes this test meaningful.
    const theme: LoadedTheme = { id: 't', name: 'T', mode: 'dark', tokens };
    expect(cssFor(theme)).toBe(cssFor(theme));
  });

  it('produces a rule with no declarations for an empty theme', () => {
    // A theme that defines nothing is valid — the Rust validator permits an empty
    // token map — and must not produce syntactically broken CSS.
    const css = cssFor({ id: 't', name: 'T', mode: 'dark', tokens: {} });
    expect(css).toContain(':root:not([data-theme])');
    expect(css.trim().endsWith('}')).toBe(true);
  });
});
