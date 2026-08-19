// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Runtime theme loading via constructible stylesheets (SPEC-V1 §7.6, AC18).
 *
 * A user theme is a named set of custom-property values, applied without a reload.
 * There are three ways to get CSS into a page at runtime and only one of them works
 * here:
 *
 * | Approach | Why not |
 * |---|---|
 * | `<style>` element | Inline stylesheet. Blocked — the production CSP is `style-src 'self'` with no `unsafe-inline`, and relaxing it to load a theme would relax it for an injected `<style>` too. |
 * | `element.style.setProperty` per token | Works, but writes hundreds of inline styles onto `<html>`, which `style-src` also governs in stricter configurations, and leaves no way to remove a theme cleanly. |
 * | `CSSStyleSheet` + `adoptedStyleSheets` | **This.** CSSOM is not a fetch and not an inline stylesheet, so CSP does not apply. Replacing the array is atomic and reversible. |
 *
 * That is what AC18 checks, on both webviews, and the reason it is a criterion at
 * all: a theme system that needs `unsafe-inline` has traded a real XSS mitigation
 * for a cosmetic feature.
 *
 * ## Trust boundary
 *
 * **Nothing here validates anything.** A theme reaching this module has already
 * been through `services::theme::validate` in Rust, which admits custom-property
 * declarations only, matched against a grammar that cannot express a fetch —
 * `url()`, `image-set()` and every escaped spelling of them are rejected there.
 * CSP is the second layer and this module is neither: it is the applicator.
 *
 * If you are adding a path that reaches `apply` with CSS that did not come from
 * `theme_import`, stop. That is the bug this comment exists to prevent.
 */

import type { ResolvedTheme } from './mode';

/** A validated theme, as `theme_list` and `theme_import` return it. */
export interface LoadedTheme {
  /** Stable id, also the `app_state.theme_id` value. */
  readonly id: string;
  /** Display name. */
  readonly name: string;
  /** Which built-in mode this theme replaces. */
  readonly mode: ResolvedTheme;
  /** Custom-property name to value. Already validated in Rust. */
  readonly tokens: Readonly<Record<string, string>>;
}

/**
 * The sheet this module owns.
 *
 * One sheet, reused. Constructing a new one per swap would leak them into the
 * adopted array and make "remove the current theme" mean "work out which of these
 * was mine".
 */
let sheet: CSSStyleSheet | null = null;

/** Whether the browser supports the only CSP-compatible mechanism we have. */
export function isSupported(): boolean {
  return (
    typeof CSSStyleSheet === 'function' &&
    typeof CSSStyleSheet.prototype.replaceSync === 'function' &&
    'adoptedStyleSheets' in Document.prototype
  );
}

/**
 * Serialise a theme to a single rule.
 *
 * Scoped to `:root` with an attribute selector so a theme replacing `light` does
 * not apply while dark is active. Specificity beats the token layer's bare `:root`
 * and matches `[data-theme="light"]`, so declaration order stops mattering.
 *
 * Values are already validated; they are not escaped again here, because escaping
 * validated CSS would corrupt legitimate values like `rgba(0, 0, 0, .2)` and would
 * imply this layer is a security boundary when it is not.
 */
function toCss(theme: LoadedTheme): string {
  const selector = theme.mode === 'light' ? ':root[data-theme="light"]' : ':root:not([data-theme])';
  const body = Object.entries(theme.tokens)
    .map(([name, value]) => `  ${name}: ${value};`)
    .join('\n');
  return `${selector} {\n${body}\n}\n`;
}

/**
 * Apply a theme, replacing whatever this module applied before.
 *
 * Idempotent: applying the same theme twice produces the same sheet. Rust returns
 * `tokens` sorted for exactly that reason.
 *
 * @returns `false` if constructible stylesheets are unavailable, so the caller can
 * fall back to the built-in theme rather than silently doing nothing.
 */
export function apply(theme: LoadedTheme): boolean {
  if (!isSupported()) return false;

  sheet ??= new CSSStyleSheet();
  sheet.replaceSync(toCss(theme));

  if (!document.adoptedStyleSheets.includes(sheet)) {
    // A new array rather than `.push`: `adoptedStyleSheets` is a live
    // FrozenArray in older implementations, where mutation throws.
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  }
  return true;
}

/** Remove the applied theme, returning to the built-in tokens. */
export function clear(): void {
  if (!sheet) return;
  sheet.replaceSync('');
  document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== sheet);
  sheet = null;
}

/**
 * Snapshot the built-in palette, if nothing is overriding it.
 *
 * Called from the store at hydrate, before any imported theme is applied — the one
 * moment the built-in values are guaranteed to be the ones on the root. Without it, a
 * session that launches with an imported theme already active would have nothing to
 * draw the "Built-in" swatch from.
 */
export function noteBuiltIn(): void {
  if (!hasTheme()) captureBuiltIn();
}

/**
 * The CSS a theme would produce, for tests.
 *
 * Exported so the E2E assertion for AC18 can compare what was adopted against what
 * was asked for, rather than inferring it from a computed colour.
 *
 * @internal
 */
export function cssFor(theme: LoadedTheme): string {
  return toCss(theme);
}

/**
 * The sheet holding the imported-theme swatches.
 *
 * Separate from {@link sheet} because the two have different lifetimes: the applied
 * theme changes when the user picks one, the swatch set changes when they import or
 * remove one, and sharing a sheet would make each update clobber the other.
 */
let swatchSheet: CSSStyleSheet | null = null;

/** The built-in palette's own swatch colours, snapshotted while it was in force. */
let builtIn: { background: string; accent: string } | null = null;

/**
 * Remember what the built-in palette looks like, for the "Built-in" swatch.
 *
 * Necessary because an imported theme redefines `--accent` and `--surface-app` **on
 * the root**, so a swatch that reads them live shows the active theme's colours under
 * the label "Built-in" — telling the user that going back would change nothing, right
 * when they are deciding whether to. Custom properties inherit, so there is no
 * selector that escapes the override; the value has to be captured while it is still
 * true.
 *
 * Call only when no imported theme is applied. {@link applySwatches} does that check
 * itself, and {@link clear} is the other moment it is guaranteed.
 */
function captureBuiltIn(): void {
  const computed = getComputedStyle(document.documentElement);
  const background = computed.getPropertyValue('--surface-app').trim();
  const accent = computed.getPropertyValue('--accent').trim();
  if (background !== '' && accent !== '') builtIn = { background, accent };
}

/** Whether this module currently has a theme applied. */
function hasTheme(): boolean {
  return sheet !== null && sheet.cssRules.length > 0;
}

/**
 * Publish each theme's own colours as custom properties, keyed by list position.
 *
 * The settings list draws a swatch per imported theme, and the honest colours for
 * that swatch are the theme's own — a preview painted in the current palette tells
 * the user nothing about what they are about to pick. Those values are data, so they
 * cannot be a class in the token layer, and the React `style` prop is banned here for
 * a good reason: the production CSP is `style-src 'self'`, so inline styles survive
 * `pnpm dev` and vanish from the packaged build. That is the worst shape a bug can
 * have, and it is the whole reason this goes through CSSOM like everything else.
 *
 * Keyed by **index**, not by theme id. An id is user-supplied and may contain spaces
 * and quotes ([`is_safe_identity`] admits both), so interpolating one into an
 * attribute selector would need escaping to stay correct. An index is a number this
 * module generates.
 *
 * @param themes - The imported themes, in the order the list renders them.
 */
export function applySwatches(themes: readonly LoadedTheme[]): void {
  if (!isSupported()) return;

  // Only while the built-in palette is actually in force, or the snapshot would
  // record whichever imported theme happens to be applied.
  if (!hasTheme()) captureBuiltIn();

  swatchSheet ??= new CSSStyleSheet();
  const builtInRule =
    builtIn === null
      ? ''
      : `[data-swatch="builtin"] {
  --swatch-bg: ${builtIn.background};
  --swatch-accent: ${builtIn.accent};
}
`;
  const css = themes
    .map((theme, index) => {
      // A theme may set as few as one token, so both fall back to the palette in
      // force rather than rendering a swatch with no colour in it.
      const background = theme.tokens['--surface-app'] ?? 'var(--surface-raised)';
      const accent = theme.tokens['--accent'] ?? 'var(--accent)';
      return `[data-swatch="${String(index)}"] {\n  --swatch-bg: ${background};\n  --swatch-accent: ${accent};\n}\n`;
    })
    .join('');
  swatchSheet.replaceSync(builtInRule + css);

  if (!document.adoptedStyleSheets.includes(swatchSheet)) {
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, swatchSheet];
  }
}
