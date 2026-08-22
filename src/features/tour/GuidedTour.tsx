// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The anchored sequence — HO-002's `GuidedTour`, as a component.
 *
 * Same DOM, same class names, same `data-side`, same lifecycle. All of the
 * appearance and all of the motion is `guided-tour.css`, vendored byte-identical;
 * this file is the controller, rewritten in React because the sequence has to
 * read the app's navigation store and write `app_state` through typed IPC.
 *
 * ## The card travels; it does not dismiss and re-enter
 *
 * MOTION.md is explicit and it is the opposite of what this repository had
 * before: *"Four entrances and four exits read as four separate interruptions.
 * One card that moves reads as a guide walking you through."* So the card is
 * mounted once for the whole sequence, keeps its shadow, and slides to the next
 * anchor over `--dur-move` with the ring moving in lockstep and only the text
 * block crossfading.
 *
 * Two implementation notes the handoff calls out, because both are invisible
 * until they are wrong:
 *
 * - **The entrance animation is removed once it has played** (`gt-card--enter` →
 *   `gt-card--settled`). Leave it on and the next step's `data-side` change
 *   replays the arrival, so the card appears to teleport.
 * - **The copy crossfade alternates between two identical keyframe sets.** A CSS
 *   animation only replays when its `animation-name` changes, and remounting the
 *   element instead would cost the continuity that is the whole point.
 *
 * ## Each step selects the view it describes
 *
 * INTEGRATION.md §4: *"Reading about the generator while looking at the
 * generator is most of the value; reading about it while looking at a list is a
 * tooltip with extra steps."* So a step navigates. It does not block — nothing
 * is disabled, there is no scrim, and the layer is `pointer-events: none` apart
 * from the card itself.
 *
 * ## Coordinates
 *
 * `place` works in frame-relative CSS pixels. `getBoundingClientRect` does not:
 * the shell sets `zoom` on the root, and CSS `zoom` scales a rect while leaving
 * a `left:` declaration in the same tree unscaled. Every measurement is divided
 * by {@link frameZoom} on the way in. This is the one thing the handoff's module
 * does not do, because it does not know about the zoom; without it every card
 * lands a fifth of the window out.
 *
 * ## Why the positions are written imperatively
 *
 * `guided-tour.css` transitions `left` and `top` on the card and `left`, `top`,
 * `width` and `height` on the ring, so those are the properties that have to
 * carry the value. They are set through CSSOM, which is what the eslint rule in
 * this repository permits — it bans the JSX `style` *attribute*, because a markup
 * `style=""` is dropped under the production CSP's `style-src 'self'`. A script
 * assigning to `element.style` is not an inline stylesheet and is unaffected;
 * `theme/loader.ts` documents the same distinction.
 */

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';

import { cn } from '../../lib/cn';
import { APP_TOUR } from './content';
import type { TourStep } from './content';
import { CARD_W, place } from './place';
import type { Side } from './place';
import { XMark } from './Notice';
import { useTour } from './store';
import { useNavigation } from '../../app/navigation';

/** HO-002: the entrance is frozen at this point so a step change moves the card. */
const SETTLE_MS = 280;
const SETTLE_MS_REDUCED = 130;

/** HO-002: `DUR_OUT + 10`, so the final frame is never clipped. */
const OUT_MS = 170;
const OUT_MS_REDUCED = 110;

/** `--row-toolbar`. The title bar the card must never tuck under. */
const TOP_INSET_FALLBACK = 52;

/**
 * How long a freshly opened step keeps re-reading its anchor, and how many still
 * frames end that early.
 *
 * A surface arrives under `animate-pane-in`, which slides it up four pixels over
 * `--duration-moderate`. Placement runs on frame two, about 30ms in, so a ring
 * written once and left alone lands four pixels low: the top row of a stat grid
 * finishes flush against the inside of the ring instead of four pixels in from it.
 * Neither observer below can see it — the pane translates, it does not resize and
 * it does not change the tree.
 *
 * So the anchor is re-read every frame until it holds still. Twenty-odd frames of
 * four `getBoundingClientRect` calls, once per step, and it ends as soon as the
 * rect stops moving rather than running the clock out.
 */
const SETTLE_WATCH_MS = 360;
const SETTLE_STILL_FRAMES = 3;

function prefersReducedMotion(): boolean {
  return (
    typeof globalThis.matchMedia === 'function' &&
    globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/**
 * The scale between the frame's CSS pixels and the ones its rect is reported in.
 *
 * `getBoundingClientRect` includes CSS `zoom`; `clientWidth` does not. Their
 * ratio is therefore the effective zoom, needs no feature detection, and works
 * on any engine — which matters, because `currentCSSZoom` is recent and the
 * macOS build cannot be compiled here to find out what WKWebView has (ADD-005).
 *
 * Returns `1` when there is no layout to measure, so a missing measurement
 * degrades to unscaled rather than to `NaN`.
 */
function frameZoom(frame: Element): number {
  const measured = frame.getBoundingClientRect().width;
  const declared = frame.clientWidth;
  if (declared <= 0 || measured <= 0) return 1;
  return measured / declared;
}

/**
 * Bring an anchor into view by scrolling its own container, not the page.
 *
 * INTEGRATION.md §3: *"Do not anchor to something below the fold. Placement does
 * not scroll. If the step's subject needs scrolling to reach, scroll it into view
 * in `onEnter` first (via a container `scrollTop`, not `scrollIntoView`)."*
 *
 * The backup row is the case that needs it — it is most of a screen down the
 * settings pane, and without this the ring lands off the bottom of the frame and
 * the beak points at nothing. Rendered and measured: this is not speculative.
 *
 * `scrollTo` with `behavior: 'instant'` rather than an assignment to `scrollTop`,
 * because the app's scroll panes carry `scroll-behavior: smooth` — an assignment
 * would animate, and placement runs on the next frame, against a position the
 * pane has not reached yet.
 */
function revealInContainer(anchor: Element): void {
  const pane = anchor.closest('[data-scroll-pane]');
  if (!(pane instanceof HTMLElement)) return;

  const paneBox = pane.getBoundingClientRect();
  const box = anchor.getBoundingClientRect();
  // A third of the pane above the anchor, so the card has somewhere to go below
  // it and the row does not sit against the top edge looking like a header.
  const headroom = paneBox.height / 3;
  if (box.top >= paneBox.top + headroom && box.bottom <= paneBox.bottom) return;

  pane.scrollTo({ top: pane.scrollTop + (box.top - paneBox.top) - headroom, behavior: 'instant' });
}

/** An anchor's box as a comparable string: has it moved, or resized, since? */
function rectKey(target: Element | null): string {
  if (target === null) return '';
  const r = target.getBoundingClientRect();
  return `${String(r.left)},${String(r.top)},${String(r.width)},${String(r.height)}`;
}

/** A length token off the root, in px, for the arithmetic that needs a number. */
function lengthToken(name: string, fallback: number): number {
  if (typeof globalThis.getComputedStyle !== 'function') return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const parsed = Number.parseFloat(raw);
  return raw.endsWith('px') && Number.isFinite(parsed) ? parsed : fallback;
}

/** The three elements a placement writes to. */
interface TourNodes {
  readonly card: HTMLDivElement;
  readonly ring: HTMLDivElement;
  readonly beak: HTMLDivElement;
}

/**
 * Measure the step's anchor and write every position — HO-002's `_place`, with the
 * zoom divided out and the ring sized from the anchor's own border box.
 *
 * Returns the side the card ended up on, or `null` when the anchor is not in the
 * layout. The caller leaves the card where it was in that case: there is nothing
 * honest to point at, and moving it would only move the beak somewhere equally
 * arbitrary.
 *
 * `reveal` scrolls the anchor into view. True when the step opens or the anchor is
 * new, false on a re-measure — re-asserting the scroll every time the pane mutates
 * would drag the view back under someone who scrolled away to read.
 *
 * At module scope rather than in a `useCallback`, because it reads nothing from the
 * component but the nodes it is handed and one effect is its only caller.
 */
function measureAndPlace(
  host: HTMLElement,
  nodes: TourNodes,
  step: TourStep,
  reveal: boolean,
): Side | null {
  const target = host.querySelector(`[data-tour="${step.anchor}"]`);
  if (!target) return null;

  if (reveal) revealInContainer(target);

  const zoom = frameZoom(host);
  const css = (value: number) => value / zoom;

  const frameRect = host.getBoundingClientRect();
  // The anchor's border box, which is the whole subject: every step's anchor is a
  // wrapper, so a stat grid that has wrapped to two rows measures both rows here
  // and the ring gets all of it. `place` never sizes the ring; it positions the
  // card, and it is handed the same rect.
  const anchorRect = target.getBoundingClientRect();
  const a = {
    l: css(anchorRect.left - frameRect.left),
    t: css(anchorRect.top - frameRect.top),
    w: css(anchorRect.width),
    h: css(anchorRect.height),
  };
  const size = { w: css(frameRect.width), h: css(frameRect.height) };
  const ch = Math.round(css(nodes.card.getBoundingClientRect().height)) || 176;

  const p = place(a, size, CARD_W, ch, step.side, lengthToken('--row-toolbar', TOP_INSET_FALLBACK));

  nodes.card.style.left = `${String(p.left)}px`;
  nodes.card.style.top = `${String(p.top)}px`;

  // Transform origin on the beak, so the entrance grows out of the thing the card
  // points at rather than out of its own centre. MOTION.md calls this "most of the
  // reason the anchoring reads as intentional".
  if (p.side === 'right' || p.side === 'left') {
    nodes.card.style.transformOrigin = `${p.side === 'right' ? '0px' : `${String(CARD_W)}px`} ${String(p.beak)}px`;
    nodes.beak.style.left = p.side === 'right' ? '-7px' : `${String(CARD_W - 7)}px`;
    nodes.beak.style.top = `${String(p.beak - 7)}px`;
  } else {
    nodes.card.style.transformOrigin = `${String(p.beak)}px ${p.side === 'bottom' ? '0px' : `${String(ch)}px`}`;
    nodes.beak.style.left = `${String(p.beak - 7)}px`;
    nodes.beak.style.top = p.side === 'bottom' ? '-7px' : `${String(ch - 7)}px`;
  }

  nodes.ring.style.left = `${String(Math.round(a.l - step.ringPad))}px`;
  nodes.ring.style.top = `${String(Math.round(a.t - step.ringPad))}px`;
  nodes.ring.style.width = `${String(Math.round(a.w + step.ringPad * 2))}px`;
  nodes.ring.style.height = `${String(Math.round(a.h + step.ringPad * 2))}px`;
  nodes.ring.style.borderRadius = `${String(step.ringRadius)}px`;

  return p.side;
}

export interface GuidedTourProps {
  /**
   * The positioning context: the app window, not the viewport.
   *
   * INTEGRATION.md §2 — *"the tour should be bounded by your chrome, not the
   * viewport"*. `WindowFrame` is already `relative` and `overflow-hidden`.
   *
   * The ref object rather than the element, because the element is only there
   * after the first commit and nothing re-renders when it arrives. Every read is
   * inside an effect, where a ref is meant to be read.
   */
  frame: RefObject<HTMLDivElement | null>;
  /**
   * Suppress the tour without ending it.
   *
   * True while a sheet or the palette is open. The step is kept, so the card
   * returns where it left off rather than a modal the user opened for an
   * unrelated reason counting as a skip.
   */
  paused: boolean;
  /** `Ctrl` or `⌘`, from `app_platform_info`. Never hardcoded (SPEC-V1 §8). */
  modifierKey: string;
}

export function GuidedTour({ frame, paused, modifierKey }: GuidedTourProps) {
  const step = useTour((s) => s.step);
  const advance = useTour((s) => s.advance);
  const finish = useTour((s) => s.finish);
  const go = useNavigation((s) => s.go);

  const card = useRef<HTMLDivElement>(null);
  const ring = useRef<HTMLDivElement>(null);
  const beak = useRef<HTMLDivElement>(null);
  const cta = useRef<HTMLButtonElement>(null);
  const restoreTo = useRef<Element | null>(null);
  const exitTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [phase, setPhase] = useState<'enter' | 'settled' | 'exit'>('enter');
  // Alternates so the crossfade restarts. See the note above: a CSS animation
  // replays only when its `animation-name` changes.
  const [parity, setParity] = useState(0);
  const [side, setSide] = useState<'right' | 'left' | 'bottom' | 'top'>('right');

  const current = APP_TOUR[step];
  const last = step === APP_TOUR.length - 1;

  /** Select the view this step describes, before anything is measured. */
  useLayoutEffect(() => {
    if (paused || !current) return;
    go(current.surface);
  }, [paused, current, go]);

  /**
   * Place the card and the ring, and keep them placed for as long as the step is open.
   *
   * HO-002 measures once, on the second frame after the step changes. That is right
   * for its demo, where every anchor is in the layout from the first paint and never
   * moves afterwards. It is not enough here, and step 3 is the proof: the security
   * pane renders an empty `<section>` while `security_report_run` is in flight, so
   * `[data-tour="security"]` does not exist on the frame the measurement happens.
   * `measureAndPlace` returns `null` — correctly, there is nothing to point at — the
   * stat cards arrive about 120ms later, and under the old code nothing re-measured.
   *
   * The ring was then still on step 2's box, which is close enough to the first row
   * of five stat cards to be misread as a ring that lost the second row. It never
   * measured a row. It never measured the grid at all.
   *
   * So the first placement still waits two frames for the view swap to land, and
   * after that:
   *
   * - a `ResizeObserver` on the anchor follows its box — a second grid row arriving,
   *   a card growing a line when the window narrows, fonts settling;
   * - a `MutationObserver` on the frame follows the anchor itself — appearing late,
   *   being replaced by React, or being pushed down the pane by content rendering
   *   above it, none of which changes the anchor's own size;
   * - and for the first {@link SETTLE_WATCH_MS} after the step opens or its anchor
   *   arrives, the anchor is re-read every frame until it holds still, because a
   *   surface arrives translating and neither observer can see a translate.
   *
   * The mutation callback coalesces onto one frame, and neither observer watches
   * attributes, so the inline styles a placement writes cannot feed back into them.
   */
  useEffect(() => {
    const host = frame.current;
    if (paused || !current || !host) return undefined;

    const selector = `[data-tour="${current.anchor}"]`;
    let observed: Element | null = null;
    let queued = 0;
    let watch = 0;
    let placed = '';
    // Declared before `sync` so the two can refer to each other without a
    // use-before-define; neither runs until the first frame, by which point both
    // are bound.
    let size: ResizeObserver | null = null;

    /** Re-place, and re-point the size observer if the anchor is a different node. */
    const sync = (reveal: boolean) => {
      const node = card.current;
      const ringNode = ring.current;
      const beakNode = beak.current;
      if (!node || !ringNode || !beakNode) return;

      const target = host.querySelector(selector);
      if (target !== observed) {
        if (observed !== null) size?.unobserve(observed);
        observed = target;
        if (target !== null) size?.observe(target);
      }

      placed = rectKey(target);
      const chosen = measureAndPlace(
        host,
        { card: node, ring: ringNode, beak: beakNode },
        current,
        reveal,
      );
      if (chosen !== null) setSide(chosen);
    };

    /**
     * Re-place every frame until the anchor's box stops changing.
     *
     * Called at the two moments the subject is new — the step opening and the
     * anchor arriving — which are the two moments it is most likely to still be
     * moving. A const rather than a hoisted `function`, so it can close over
     * `sync` and so TypeScript keeps the non-null narrowing on `host`.
     */
    const watchUntilStill = () => {
      if (watch !== 0) return;
      const deadline = performance.now() + SETTLE_WATCH_MS;
      let still = 0;
      const tick = (now: number) => {
        const key = rectKey(host.querySelector(selector));
        if (key === placed) {
          still += 1;
        } else {
          still = 0;
          sync(false);
        }
        watch = still >= SETTLE_STILL_FRAMES || now >= deadline ? 0 : requestAnimationFrame(tick);
      };
      watch = requestAnimationFrame(tick);
    };

    size =
      typeof ResizeObserver === 'function'
        ? new ResizeObserver(() => {
            sync(false);
          })
        : null;

    const tree =
      typeof MutationObserver === 'function'
        ? new MutationObserver(() => {
            if (queued !== 0) return;
            // A new anchor earns a scroll into view; one that merely moved does not,
            // or the pane would jump under someone reading it.
            const arriving = observed === null || !observed.isConnected;
            queued = requestAnimationFrame(() => {
              queued = 0;
              sync(arriving);
              if (arriving) watchUntilStill();
            });
          })
        : null;
    tree?.observe(host, { childList: true, subtree: true });

    const onResize = () => {
      // The frame changed, not the anchor: `place` clamps against the frame, so the
      // card can need moving even when the anchor's box is identical.
      sync(false);
    };
    globalThis.addEventListener('resize', onResize);

    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => {
        sync(true);
        watchUntilStill();
        cta.current?.focus({ preventScroll: true });
      });
    });

    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
      if (queued !== 0) cancelAnimationFrame(queued);
      if (watch !== 0) cancelAnimationFrame(watch);
      tree?.disconnect();
      size?.disconnect();
      globalThis.removeEventListener('resize', onResize);
    };
  }, [paused, current, frame]);

  // Freeze the entrance so a step change moves the card instead of replaying it.
  useEffect(() => {
    const timer = setTimeout(
      () => {
        setPhase((p) => (p === 'enter' ? 'settled' : p));
      },
      prefersReducedMotion() ? SETTLE_MS_REDUCED : SETTLE_MS,
    );
    return () => {
      clearTimeout(timer);
    };
  }, []);

  // Where focus was before the tour took it, so it can be given back.
  useEffect(() => {
    restoreTo.current = document.activeElement;
    return () => {
      const node = restoreTo.current;
      if (node instanceof HTMLElement) node.focus({ preventScroll: true });
    };
  }, []);

  useEffect(
    () => () => {
      if (exitTimer.current !== null) clearTimeout(exitTimer.current);
    },
    [],
  );

  const leave = useCallback(() => {
    if (exitTimer.current !== null) return;
    setPhase('exit');
    exitTimer.current = setTimeout(finish, prefersReducedMotion() ? OUT_MS_REDUCED : OUT_MS);
  }, [finish]);

  useEffect(() => {
    if (paused) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      leave();
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
  }, [paused, leave]);

  if (paused || !current) return null;

  const onNext = () => {
    if (last) {
      leave();
      return;
    }
    setParity((p) => 1 - p);
    advance();
  };

  return (
    <div className="gt-layer">
      <div ref={ring} className={cn('gt-ring', phase === 'exit' && 'gt-ring--exit')} />
      <div
        ref={card}
        className={cn('gt-card', 'gt-card--' + phase)}
        data-side={side}
        role="dialog"
        // Not modal, and deliberately: the app stays interactive behind the card.
        aria-modal="false"
        aria-labelledby="gt-title"
      >
        <div ref={beak} className="gt-beak" />
        <div className={cn('gt-swap', parity ? 'gt-swap--b' : 'gt-swap--a')}>
          <div className="gt-head">
            {/* The worded position, which is also the accessible progress
                indicator — the dot row below is aria-hidden. */}
            <span className="gt-eyebrow">{current.eyebrow}</span>
            <button type="button" className="gt-close" aria-label="End tour" onClick={leave}>
              <XMark />
            </button>
          </div>
          <h2 className="gt-title" id="gt-title">
            {current.title}
          </h2>
          <p className="gt-body">{current.body(modifierKey)}</p>
          <div className="gt-foot">
            <div className="gt-dots" aria-hidden="true">
              {APP_TOUR.map((s, i) => (
                <span
                  key={s.id}
                  className={cn('gt-dot', i === step && 'gt-dot--on', i < step && 'gt-dot--past')}
                />
              ))}
            </div>
            <button type="button" className="gt-skip" onClick={leave}>
              Skip
            </button>
            <button ref={cta} type="button" className="gt-cta" onClick={onNext}>
              {last ? 'Done' : 'Next'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
