// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * A block of a computed pixel height, for list virtualisation.
 *
 * ## Why this is imperative
 *
 * A virtual list needs two spacers whose heights are arithmetic, not design values.
 * The obvious way to express that is the React `style` prop, and it is **banned
 * repo-wide by an eslint rule** because SPEC-V1 §7.6 requires it:
 *
 * > `style-src 'self'` blocks injected `<style>` and markup `style=""`. … Ban the
 * > React `style` prop repo-wide with an eslint rule. Otherwise someone adds one and
 * > the CSP silently drops it in release only.
 *
 * That last clause is the whole problem: a `style` prop works in `pnpm dev`, where
 * the dev CSP is looser, and is dropped in the release build. The list would scroll
 * correctly for every developer and be broken for every user.
 *
 * §7.6 names the sanctioned alternative in the same breath —
 * *"`element.style.setProperty()` (CSSOM, also permitted) is the fallback"* — because
 * CSP governs markup and fetched stylesheets, not the CSSOM. So this writes the height
 * through a ref. It is the only imperative style write in the app, it is here rather
 * than scattered, and it carries no colour, radius or spacing: a height in pixels
 * derived from a row count is not a design decision.
 */

import { useCallback } from 'react';

export interface SpacerProps {
  /** Height in pixels. Rounded, because a fractional row offset shows as a seam. */
  height: number;
}

export function Spacer({ height }: SpacerProps) {
  const ref = useCallback(
    (element: HTMLDivElement | null) => {
      if (element) element.style.height = `${String(Math.max(0, Math.round(height)))}px`;
    },
    [height],
  );

  return <div ref={ref} aria-hidden="true" />;
}
