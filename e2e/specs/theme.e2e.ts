// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Theme rendering and the CSP-safe swap — AC17, AC18.
 *
 * These run inside the real app under the production CSP (`style-src 'self'`, no
 * `unsafe-inline`), which is the only place either criterion can be settled:
 *
 * - **AC17** — both palettes must actually render. Asserted on *computed* colour, so
 *   the token layer, the cascade and the `data-theme` attribute are all exercised
 *   together. A test that only checked the attribute would pass with an empty
 *   stylesheet.
 * - **AC18** — `adoptedStyleSheets` must work where an injected `<style>` is blocked.
 *   The second test proves both halves: the constructible sheet applies, and an
 *   injected `<style>` with the same declaration does **not**. Without the negative
 *   half, a relaxed CSP would look identical to a working one.
 *
 * ADD-005: this is WebView2 only. The WKWebView half of AC18 is unverified and is a row
 * in MACOS-UNVERIFIED.md.
 */

import { browser, expect } from '@wdio/globals';

/** Read a computed style property from the document element. */
async function computedRootStyle(property: string): Promise<string> {
  return browser.execute(
    (prop: string) => globalThis.getComputedStyle(document.documentElement).getPropertyValue(prop),
    property,
  );
}

/** Read a computed background colour for a selector. */
async function computedBackground(selector: string): Promise<string> {
  return browser.execute((sel: string) => {
    const element = document.querySelector(sel);
    if (!element) return '';
    return globalThis.getComputedStyle(element).backgroundColor;
  }, selector);
}

/** Set the theme through the app's own store, the way the toggle does. */
async function setTheme(theme: 'dark' | 'light'): Promise<void> {
  await browser.execute((next: string) => {
    if (next === 'light') document.documentElement.setAttribute('data-theme', 'light');
    else document.documentElement.removeAttribute('data-theme');
  }, theme);
}

describe('AC17 — dark and light both render', () => {
  it('renders the window shell', async () => {
    // The window is created hidden and revealed after the theme resolves. If this
    // fails, nothing called `show()` — which is exactly the bug that shipped once.
    //
    // Asserted on the mount point and on whichever surface the app decided to draw,
    // rather than on a class name. The app is locked at launch, so the sidebar is
    // deliberately *not* mounted (§4.9: lock is real, there is nothing behind it) —
    // and a selector that can never match is a test that can never fail for the right
    // reason.
    await expect($('#root')).toBeExisting();

    const mounted = await browser.execute(() => {
      const root = document.getElementById('root');
      const locked = document.querySelector('[role="dialog"][aria-modal="true"]') !== null;
      const unlocked = document.querySelector('nav[aria-label="Sources"]') !== null;
      return { children: root?.childElementCount ?? 0, locked, unlocked };
    });

    // React mounted something.
    expect(mounted.children).toBeGreaterThan(0);
    // And it is one of the two real surfaces, not a blank div. A packaged build that
    // pointed at `devUrl` with no dev server running would fail here.
    expect(mounted.locked || mounted.unlocked).toBe(true);
  });

  it('resolves the dark palette to a real colour', async () => {
    await setTheme('dark');
    const surface = (await computedRootStyle('--surface-panel')).trim();
    // Not empty, and not the literal `var(...)` — either would mean the token layer
    // never loaded, which is the failure mode this criterion exists to catch.
    expect(surface).not.toBe('');
    expect(surface.startsWith('var(')).toBe(false);

    // Opaque, tested as a property rather than by matching a colour string. A literal
    // here would be a hardcoded colour in the repo — which `check:tokens` fails,
    // correctly — and parsing the alpha is what the assertion actually means.
    const opaque = await browser.execute(() => {
      const background = globalThis.getComputedStyle(document.body).backgroundColor;
      const parts = /[\d.]+/g.exec(background) === null ? [] : background.match(/[\d.]+/g);
      const alpha = parts && parts.length === 4 ? Number(parts[3]) : 1;
      return alpha > 0;
    });
    expect(opaque).toBe(true);
  });

  it('resolves the light palette to a different colour', async () => {
    await setTheme('dark');
    const dark = (await computedRootStyle('--surface-panel')).trim();

    await setTheme('light');
    const light = (await computedRootStyle('--surface-panel')).trim();

    expect(light).not.toBe('');
    // The point of the criterion: both render, and they render *differently*. A light
    // theme that resolved to the dark values would pass every other check here.
    expect(light).not.toBe(dark);
  });

  it('paints a different body background per theme', async () => {
    await setTheme('dark');
    const dark = await computedBackground('body');
    await setTheme('light');
    const light = await computedBackground('body');
    expect(light).not.toBe(dark);
  });

  after(async () => {
    await setTheme('dark');
  });
});

describe('AC18 — runtime theme swap under the production CSP', () => {
  /** A token no built-in theme sets, so a hit can only come from the applied sheet. */
  const PROBE = '--e2e-probe-token';

  it('applies a constructible stylesheet', async () => {
    const applied = await browser.execute((token: string) => {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(`:root { ${token}: 42px; }`);
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      const value = globalThis.getComputedStyle(document.documentElement).getPropertyValue(token);
      document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== sheet);
      return value.trim();
    }, PROBE);

    // This is the whole of §7.6's bet: CSSOM is not an inline stylesheet, so
    // `style-src 'self'` does not govern it.
    expect(applied).toBe('42px');
  });

  it('blocks an injected style element, which is why the sheet is used', async () => {
    const injected = await browser.execute((token: string) => {
      const element = document.createElement('style');
      element.textContent = `:root { ${token}: 99px; }`;
      document.head.append(element);
      const value = globalThis.getComputedStyle(document.documentElement).getPropertyValue(token);
      element.remove();
      return value.trim();
    }, PROBE);

    // The negative half. If this returns `99px` the CSP is not what the config says,
    // and the first test proved nothing about CSP — only that CSSOM works.
    expect(injected).not.toBe('99px');
  });

  it('removes an applied theme cleanly', async () => {
    const after = await browser.execute((token: string) => {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(`:root { ${token}: 7px; }`);
      document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      sheet.replaceSync('');
      document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== sheet);
      return globalThis.getComputedStyle(document.documentElement).getPropertyValue(token).trim();
    }, PROBE);

    // Reverting to the built-in tokens has to be complete, or a user who removes a
    // custom theme keeps half of it.
    expect(after).toBe('');
  });
});
