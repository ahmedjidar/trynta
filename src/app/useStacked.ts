// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Whether a region is too narrow to hold the list and the detail pane side by side.
 *
 * ## Why this is not a media query
 *
 * The shell sets `zoom: 1.25` on the root. A media query is evaluated against the
 * viewport *before* element zoom, so `(min-width: 800px)` fires at 800 device
 * pixels while everything the layout is measured in — every token, every rect
 * inside the document — is 640 by then. Measured, not assumed: at a 1440px window
 * `documentElement.clientWidth` reads 1424 while the region inside it reads 899.
 * A breakpoint written in device pixels would therefore mean something else the
 * moment `src/app/zoom.ts` changed its default, and that file owns the zoom.
 *
 * A `ResizeObserver` on the region measures the same pixels the layout uses — a
 * descendant's `clientWidth` is in the zoomed context's own units — so the
 * threshold is the sum of two tokens with nothing to convert.
 *
 * ## Why the threshold is those two tokens
 *
 * `--width-list` is what the list column is, and `--width-detail-min` is the
 * narrowest the detail pane can be and still show an item's values in full; the
 * note on that token records how it was measured. Below their sum the pane is
 * being asked to truncate, which is the point the side-by-side arrangement stops
 * being worth keeping.
 *
 * ## Why a callback ref
 *
 * The region unmounts whenever the user leaves the vault for the generator or
 * settings, and mounts again on the way back. With a `RefObject` the effect runs
 * once, against whichever node existed then, and afterwards observes a detached
 * element for ever — the layout stayed in two columns at every width, which is
 * exactly the bug this hook exists to fix. Holding the node in state re-runs the
 * effect on every mount.
 */

import { useEffect, useState } from 'react';

/** A length token off the root, in px. Returns `fallback` when it is not a length. */
function lengthToken(name: string, fallback: number): number {
  if (typeof globalThis.getComputedStyle !== 'function') return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const parsed = Number.parseFloat(raw);
  return raw.endsWith('px') && Number.isFinite(parsed) ? parsed : fallback;
}

/** Used only where the tokens cannot be read, which is happy-dom in the unit tests. */
const FALLBACK_LIST = 320;
const FALLBACK_DETAIL = 483;

/**
 * Measure a region and report whether its two panes have to stack.
 *
 * Returns the flag and the ref to put on the region. `false` where there is no
 * `ResizeObserver`: the side-by-side layout is the one every existing test and
 * screenshot expects, so a missing API degrades to what was there before rather
 * than to a narrow layout nobody asked for.
 */
export function useStacked(): readonly [boolean, (node: HTMLDivElement | null) => void] {
  const [region, setRegion] = useState<HTMLDivElement | null>(null);
  const [stacked, setStacked] = useState(false);

  useEffect(() => {
    if (region === null || typeof ResizeObserver !== 'function') return undefined;

    const measure = () => {
      const threshold =
        lengthToken('--width-list', FALLBACK_LIST) +
        lengthToken('--width-detail-min', FALLBACK_DETAIL);
      // `clientWidth`, not the rect: a rect carries the root zoom and the tokens
      // do not, so comparing the two would compare 1.25× against 1×.
      setStacked(region.clientWidth > 0 && region.clientWidth < threshold);
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(region);
    return () => {
      observer.disconnect();
    };
  }, [region]);

  return [stacked, setRegion];
}
