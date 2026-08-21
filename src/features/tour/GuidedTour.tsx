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
import { CARD_W, place } from './place';
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

/** A length token off the root, in px, for the arithmetic that needs a number. */
function lengthToken(name: string, fallback: number): number {
  if (typeof globalThis.getComputedStyle !== 'function') return fallback;
  const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const parsed = Number.parseFloat(raw);
  return raw.endsWith('px') && Number.isFinite(parsed) ? parsed : fallback;
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

  /** Measure and write every position. HO-002's `_place`, with the zoom divided out. */
  const reposition = useCallback(() => {
    const host = frame.current;
    const node = card.current;
    const ringNode = ring.current;
    const beakNode = beak.current;
    if (!host || !node || !ringNode || !beakNode || !current) return;

    const target = host.querySelector(`[data-tour="${current.anchor}"]`);
    if (!target) {
      // The anchor is not in the layout. HO-002 warns and leaves the card where
      // it was; there is nothing honest to point at and moving it would only
      // move the beak somewhere equally arbitrary. Every step's anchor is
      // unconditional in its own surface, so this is a bug rather than a state.
      return;
    }

    revealInContainer(target);

    const zoom = frameZoom(host);
    const css = (value: number) => value / zoom;

    const frameRect = host.getBoundingClientRect();
    const anchorRect = target.getBoundingClientRect();
    const a = {
      l: css(anchorRect.left - frameRect.left),
      t: css(anchorRect.top - frameRect.top),
      w: css(anchorRect.width),
      h: css(anchorRect.height),
    };
    const size = { w: css(frameRect.width), h: css(frameRect.height) };
    const ch = Math.round(css(node.getBoundingClientRect().height)) || 176;

    const p = place(
      a,
      size,
      CARD_W,
      ch,
      current.side,
      lengthToken('--row-toolbar', TOP_INSET_FALLBACK),
    );

    node.style.left = `${String(p.left)}px`;
    node.style.top = `${String(p.top)}px`;
    setSide(p.side);

    // Transform origin on the beak, so the entrance grows out of the thing the
    // card points at rather than out of its own centre. MOTION.md calls this
    // "most of the reason the anchoring reads as intentional".
    if (p.side === 'right' || p.side === 'left') {
      node.style.transformOrigin = `${p.side === 'right' ? '0px' : `${String(CARD_W)}px`} ${String(p.beak)}px`;
      beakNode.style.left = p.side === 'right' ? '-7px' : `${String(CARD_W - 7)}px`;
      beakNode.style.top = `${String(p.beak - 7)}px`;
    } else {
      node.style.transformOrigin = `${String(p.beak)}px ${p.side === 'bottom' ? '0px' : `${String(ch)}px`}`;
      beakNode.style.left = `${String(p.beak - 7)}px`;
      beakNode.style.top = p.side === 'bottom' ? '-7px' : `${String(ch - 7)}px`;
    }

    ringNode.style.left = `${String(Math.round(a.l - current.ringPad))}px`;
    ringNode.style.top = `${String(Math.round(a.t - current.ringPad))}px`;
    ringNode.style.width = `${String(Math.round(a.w + current.ringPad * 2))}px`;
    ringNode.style.height = `${String(Math.round(a.h + current.ringPad * 2))}px`;
    ringNode.style.borderRadius = `${String(current.ringRadius)}px`;
  }, [frame, current]);

  /** Select the view this step describes, before anything is measured. */
  useLayoutEffect(() => {
    if (paused || !current) return;
    go(current.surface);
  }, [paused, current, go]);

  // Two frames, exactly as HO-002 does it: the view swap above has to land in
  // the layout before the anchor can be measured.
  useEffect(() => {
    if (paused) return undefined;
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => {
        reposition();
        cta.current?.focus({ preventScroll: true });
      });
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, [paused, step, reposition]);

  useEffect(() => {
    if (paused) return undefined;
    const onResize = () => {
      reposition();
    };
    globalThis.addEventListener('resize', onResize);
    return () => {
      globalThis.removeEventListener('resize', onResize);
    };
  }, [paused, reposition]);

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
