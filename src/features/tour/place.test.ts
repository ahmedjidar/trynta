// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * HO-002's placement algorithm, against the four cases ANCHORING.md draws.
 *
 * The handoff exports `place` specifically so it can be unit-tested, and these
 * are the cases its diagrams name: beside, below, clamped and flipped. Each has
 * exactly one acceptable outcome — the card inside the frame, offset from the
 * anchor, with the beak clear of the corner radius — and several that are
 * indistinguishable from the tour being broken.
 *
 * Numbers here are frame-relative CSS pixels. The zoom conversion that gets them
 * into that space lives in `GuidedTour.tsx`, deliberately outside this function
 * so the arithmetic stays pure.
 */

import { describe, expect, it } from 'vitest';

import { CARD_W, CLAMP_H, CLAMP_V, GAP, INSET, place } from './place';
import type { AnchorRect } from './place';

/** A 1152×736 frame: the app at its default size, unzoomed. */
const FRAME = { w: 1152, h: 736 };
const CH = 176;
const TOP = 52;

/** The item list: a full-height column beside the sidebar. */
const COLUMN: AnchorRect = { l: 240, t: 52, w: 320, h: 684 };

/** Assert the card is inside the frame's inset, respecting the top chrome. */
function expectInside(left: number, top: number, ch = CH, frame = FRAME): void {
  expect(left).toBeGreaterThanOrEqual(INSET);
  expect(top).toBeGreaterThanOrEqual(TOP + INSET);
  expect(left + CARD_W).toBeLessThanOrEqual(frame.w - INSET);
  expect(top + ch).toBeLessThanOrEqual(frame.h - INSET);
}

describe('place', () => {
  it('sits beside a tall anchor, offset by the gap, centred on it', () => {
    const p = place(COLUMN, FRAME, CARD_W, CH, 'right', TOP);

    expect(p.side).toBe('right');
    expect(p.left).toBe(COLUMN.l + COLUMN.w + GAP);
    // Centred across the other axis, so the beak lands mid-card.
    expect(p.top).toBe(Math.round(COLUMN.t + COLUMN.h / 2 - CH / 2));
    expect(p.beak).toBe(Math.round(CH / 2));
    expectInside(p.left, p.top);
  });

  it('never overlaps the thing it points at', () => {
    // Rule 1, checked on every side rather than asserted once.
    const cases: { anchor: AnchorRect; side: 'right' | 'left' | 'bottom' | 'top' }[] = [
      { anchor: COLUMN, side: 'right' },
      { anchor: { l: 800, t: 300, w: 200, h: 60 }, side: 'left' },
      { anchor: { l: 400, t: 100, w: 300, h: 40 }, side: 'bottom' },
      { anchor: { l: 400, t: 600, w: 300, h: 40 }, side: 'top' },
    ];

    for (const { anchor, side } of cases) {
      const p = place(anchor, FRAME, CARD_W, CH, side, TOP);
      const overlaps =
        p.left < anchor.l + anchor.w &&
        p.left + CARD_W > anchor.l &&
        p.top < anchor.t + anchor.h &&
        p.top + CH > anchor.t;
      expect(overlaps, `${side} placement overlapped its anchor`).toBe(false);
    }
  });

  it('drops below when there is no room to either side', () => {
    // The generator's output panel: a wide card in a centred pane, with less than
    // a card's width free on either flank.
    const panel: AnchorRect = { l: 344, t: 180, w: 704, h: 120 };
    const p = place(panel, FRAME, CARD_W, CH, 'bottom', TOP);

    expect(p.side).toBe('bottom');
    expect(p.top).toBe(panel.t + panel.h + GAP);
    expect(p.beak).toBe(Math.round(panel.l + panel.w / 2 - p.left));
    expectInside(p.left, p.top);
  });

  it('flips to the opposite side when the preferred one does not fit', () => {
    // Hard against the right edge: `right` cannot hold a card, `left` can.
    const anchor: AnchorRect = { l: 900, t: 300, w: 220, h: 44 };
    const p = place(anchor, FRAME, CARD_W, CH, 'right', TOP);

    expect(p.side).toBe('left');
    expect(p.left).toBe(anchor.l - GAP - CARD_W);
    expectInside(p.left, p.top);
  });

  it('falls back to the perpendicular pair when neither side fits', () => {
    // A full-width anchor: neither left nor right has room, so it goes below.
    const banner: AnchorRect = { l: 40, t: 100, w: 1072, h: 60 };
    const p = place(banner, FRAME, CARD_W, CH, 'right', TOP);

    expect(p.side).toBe('bottom');
    expectInside(p.left, p.top);
  });

  it('clamps rather than hanging off the frame, and slides the beak instead', () => {
    // An anchor low in the frame. Centring the card on it would put its bottom
    // past the fold, so the card stops at the inset and the beak takes up the
    // difference.
    const anchor: AnchorRect = { l: 240, t: 690, w: 320, h: 40 };
    const p = place(anchor, FRAME, CARD_W, CH, 'right', TOP);

    expect(p.top).toBe(FRAME.h - CH - INSET);
    expectInside(p.left, p.top);
    // Still pointing at the anchor, not at the card's own middle.
    expect(p.beak).toBeGreaterThan(CH / 2);
  });

  it('keeps the beak clear of the corner radius on both axes', () => {
    // Rule 3. The beak may never reach a corner, whatever the anchor does.
    const extremes: AnchorRect[] = [
      { l: 240, t: 52, w: 320, h: 8 },
      { l: 240, t: 728, w: 320, h: 8 },
      { l: 20, t: 300, w: 8, h: 40 },
      { l: 1124, t: 300, w: 8, h: 40 },
    ];

    for (const anchor of extremes) {
      const p = place(anchor, FRAME, CARD_W, CH, 'right', TOP);
      const clampTo = p.side === 'right' || p.side === 'left' ? CLAMP_V : CLAMP_H;
      const along = p.side === 'right' || p.side === 'left' ? CH : CARD_W;
      expect(p.beak).toBeGreaterThanOrEqual(clampTo);
      expect(p.beak).toBeLessThanOrEqual(along - clampTo);
    }
  });

  it('respects the top inset, so a card never tucks under the title bar', () => {
    const anchor: AnchorRect = { l: 240, t: 56, w: 320, h: 30 };
    const p = place(anchor, FRAME, CARD_W, CH, 'right', TOP);

    expect(p.top).toBeGreaterThanOrEqual(TOP + INSET);
  });

  it('degrades to an overlapping card rather than one off the frame', () => {
    // ANCHORING.md: "an anchor too large for any side degrades to an overlapping
    // card, never to one hanging off the frame". Damage control, and it has to be
    // this damage rather than the other.
    const huge: AnchorRect = { l: 0, t: 0, w: 1152, h: 736 };
    const p = place(huge, FRAME, CARD_W, CH, 'right', TOP);

    expectInside(p.left, p.top);
  });

  it('never returns a negative origin when the frame is smaller than the card', () => {
    const tiny = { w: 200, h: 120 };
    const p = place(COLUMN, tiny, CARD_W, CH, 'right', TOP);

    expect(p.left).toBe(INSET);
    expect(p.top).toBe(TOP + INSET);
  });
});
