// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Where the card goes — HO-002's placement algorithm, ported.
 *
 * A line-for-line port of `GuidedTour.place` from
 * `handoffs/HO-002-keyring-tsx/guided_tour/guided-tour/guided-tour.js`, with the
 * handoff's own constants and its fallback order. ANCHORING.md documents the
 * rules this encodes and says the function is exported so it can be unit-tested;
 * `place.test.ts` is that.
 *
 * Nothing here is invented and nothing is improved. The four rules, in the
 * handoff's words:
 *
 * 1. **Never on top.** The card is offset `GAP` from the anchor's edge on the
 *    axis the beak points along.
 * 2. **No scrim, no spotlight cut-out.** The ring marks the target.
 * 3. **The card clamps; the beak slides.** The card centres on the anchor across
 *    the other axis, then clamps to `INSET` from the frame. The beak takes up the
 *    difference, stopping short of the card's corners so it never collides with
 *    the 16px radius.
 * 4. **Anchors drive navigation.** Each step selects the view it describes.
 *
 * ## Coordinates are the frame's, in CSS pixels
 *
 * The handoff works in frame-relative pixels taken from `getBoundingClientRect`.
 * This app sets `zoom` on the root — 1.25 by default (`app/zoom.ts`) — and CSS
 * `zoom` is a layout property, so a rect is reported in *scaled* pixels while a
 * `left:` declaration inside the same tree is interpreted as *unscaled* ones.
 * Feeding one to the other places every card a fifth of the window out.
 *
 * That conversion is the caller's job (`GuidedTour.tsx`), so this stays pure and
 * unit-testable exactly as the handoff intends. Everything below is one
 * consistent space.
 */

/** A rectangle relative to the frame's top-left corner, in CSS pixels. */
export interface AnchorRect {
  l: number;
  t: number;
  w: number;
  h: number;
}

/** Which side of the anchor the card sits on. The beak points the other way. */
export type Side = 'right' | 'left' | 'bottom' | 'top';

/** A resolved position, frame-relative, in CSS pixels. */
export interface Placement {
  left: number;
  top: number;
  side: Side;
  /** Offset of the beak's centre along the card's pointing edge. */
  beak: number;
}

/** Anchor edge to card edge, on the axis the beak points along. */
export const GAP = 15;

/** Minimum distance from the frame's edges. */
export const INSET = 16;

/** The beak is a 14px square rotated 45°. */
export const BEAK_HALF = 7;

/** The beak stops this far from the card's top and bottom corners. */
export const CLAMP_V = 20;

/** And this far from its left and right corners. */
export const CLAMP_H = 24;

/** The card's width. Component geometry, deliberately not a design token. */
export const CARD_W = 300;

const OPPOSITE: Record<Side, Side> = {
  right: 'left',
  left: 'right',
  bottom: 'top',
  top: 'bottom',
};

/** Fallback order: the preferred side, its opposite, then the perpendicular pair. */
const PERPENDICULAR: Record<Side, readonly Side[]> = {
  right: ['bottom', 'top'],
  left: ['bottom', 'top'],
  bottom: ['right', 'left'],
  top: ['right', 'left'],
};

function clamp(value: number, lo: number, hi: number): number {
  return Math.min(Math.max(value, lo), hi);
}

/**
 * Decide where the card goes.
 *
 * @param a - The anchor, frame-relative.
 * @param frame - The frame's size.
 * @param cw - Card width.
 * @param ch - Card height, measured.
 * @param preferred - The step's preferred side. Flipped if it does not fit.
 * @param topInset - Height of fixed chrome the card must never tuck under.
 */
export function place(
  a: AnchorRect,
  frame: { w: number; h: number },
  cw: number,
  ch: number,
  preferred: Side,
  topInset = 0,
): Placement {
  const top = topInset;

  const fits: Record<Side, boolean> = {
    right: a.l + a.w + GAP + cw + INSET <= frame.w,
    left: a.l - GAP - cw - INSET >= 0,
    bottom: a.t + a.h + GAP + ch + INSET <= frame.h,
    top: a.t - GAP - ch - INSET >= top,
  };

  let side = preferred;
  if (!fits[side]) {
    for (const candidate of [OPPOSITE[side], ...PERPENDICULAR[side]]) {
      if (fits[candidate]) {
        side = candidate;
        break;
      }
    }
  }

  const loX = INSET;
  const hiX = Math.max(INSET, frame.w - cw - INSET);
  const loY = top + INSET;
  const hiY = Math.max(top + INSET, frame.h - ch - INSET);

  let left: number;
  let topPx: number;
  let beak: number;

  if (side === 'right' || side === 'left') {
    left = clamp(side === 'right' ? a.l + a.w + GAP : a.l - GAP - cw, loX, hiX);
    topPx = clamp(Math.round(a.t + a.h / 2 - ch / 2), loY, hiY);
    beak = clamp(Math.round(a.t + a.h / 2 - topPx), CLAMP_V, Math.max(CLAMP_V, ch - CLAMP_V));
  } else {
    topPx = clamp(side === 'bottom' ? a.t + a.h + GAP : a.t - GAP - ch, loY, hiY);
    left = clamp(Math.round(a.l + a.w / 2 - cw / 2), loX, hiX);
    beak = clamp(Math.round(a.l + a.w / 2 - left), CLAMP_H, Math.max(CLAMP_H, cw - CLAMP_H));
  }

  return { left: Math.round(left), top: Math.round(topPx), side, beak };
}
