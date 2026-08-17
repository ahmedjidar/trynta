/**
 * Theme mode resolution — `dark` | `light` | `system` (SPEC-V1 §7.6).
 *
 * The token layer has two value sets: `:root` is dark and `[data-theme="light"]`
 * overrides colour. Everything here does is decide which of those is active, by
 * setting or clearing one attribute on `<html>`.
 *
 * ## Why `system` is resolved here and not in CSS
 *
 * A `@media (prefers-color-scheme: light)` block in the token layer would be
 * shorter, and wrong. The stored preference is one of three values and CSS can
 * only see two, so `system` and an explicit choice would render identically while
 * the app believed they were different — and a user who picked `light` on a dark
 * OS would get dark. Resolving in one place means the attribute always says what
 * is actually on screen, which is also what makes the E2E assertion for AC17
 * possible.
 *
 * ## Why the attribute goes on `<html>` and not `<body>`
 *
 * `color-scheme` on `:root` is what tells the engine to paint native scrollbars,
 * form controls and the canvas background for the right theme. On `<body>` it
 * applies too late and the window flashes the wrong colour on launch.
 */

/** The three settings a user can choose between. */
export type ThemeMode = 'dark' | 'light' | 'system';

/** What `system` actually resolved to. Only ever `dark` or `light`. */
export type ResolvedTheme = 'dark' | 'light';

/** The media query `system` mode follows. */
const LIGHT_QUERY = '(prefers-color-scheme: light)';

/** Whether a value is one of the three modes, for reading untrusted app state. */
export function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'dark' || value === 'light' || value === 'system';
}

/**
 * What the OS currently prefers.
 *
 * Defaults to `dark` when `matchMedia` is unavailable, matching the token layer's
 * base theme — so a missing API renders the designed default rather than a theme
 * that only exists as an override.
 */
export function systemTheme(): ResolvedTheme {
  if (typeof globalThis.matchMedia !== 'function') return 'dark';
  return globalThis.matchMedia(LIGHT_QUERY).matches ? 'light' : 'dark';
}

/** Resolve a mode to the theme that should actually render. */
export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  return mode === 'system' ? systemTheme() : mode;
}

/**
 * Apply a resolved theme to the document.
 *
 * `dark` clears the attribute rather than setting `data-theme="dark"`, because the
 * dark values live on bare `:root`. Setting an attribute that no selector matches
 * would work by accident today and break the moment someone adds a
 * `[data-theme="dark"]` block.
 */
export function applyTheme(theme: ResolvedTheme): void {
  const root = document.documentElement;
  if (theme === 'light') {
    root.setAttribute('data-theme', 'light');
  } else {
    root.removeAttribute('data-theme');
  }
}

/**
 * Watch the OS preference.
 *
 * Only meaningful while the mode is `system`; the caller is responsible for
 * unsubscribing when it is not. Returns a disposer.
 *
 * @example
 * ```ts
 * const stop = watchSystemTheme((theme) => applyTheme(theme));
 * // later
 * stop();
 * ```
 */
export function watchSystemTheme(onChange: (theme: ResolvedTheme) => void): () => void {
  if (typeof globalThis.matchMedia !== 'function') return () => {};
  const query = globalThis.matchMedia(LIGHT_QUERY);
  const handler = (event: MediaQueryListEvent) => {
    onChange(event.matches ? 'light' : 'dark');
  };
  query.addEventListener('change', handler);
  return () => {
    query.removeEventListener('change', handler);
  };
}
