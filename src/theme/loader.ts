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
