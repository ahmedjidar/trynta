// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The sequence's behaviour, including the parts the handoff changed.
 *
 * Three of these assert something this repository previously did the other way
 * round, so they are worth reading as a record of the design decision rather than
 * as coverage:
 *
 * - **The card is mounted once and travels.** MOTION.md: four entrances read as
 *   four interruptions. So there is one `[role=dialog]` for the whole sequence
 *   and Next must not remount it.
 * - **Each step selects the view it describes.** INTEGRATION.md §4. Reading about
 *   the generator while looking at a list is a tooltip with extra steps.
 * - **Skip and close both end the tour.** HO-002 has one exit, reported with a
 *   reason; there is no advance-on-close.
 */

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRef } from 'react';

import { APP_TOUR } from './content';
import { GuidedTour } from './GuidedTour';
import { useTour } from './store';
import { useNavigation } from '../../app/navigation';

const tourMarkSeen = vi.hoisted(() => vi.fn());
const tourReset = vi.hoisted(() => vi.fn());
const tourState = vi.hoisted(() => vi.fn());
vi.mock('../../ipc', () => ({ tourMarkSeen, tourReset, tourState }));

/** The card's title, which is what changes between steps. */
function title(): string {
  return screen.getByRole('heading', { level: 2 }).textContent;
}

/** Mount the sequence over a frame carrying all four anchors. */
function start(step = 0) {
  useTour.setState({ ready: true, showUnlock: false, showApp: true, replay: false, step });
  useNavigation.setState({ surface: 'vault' });
  const frame = createRef<HTMLDivElement>();
  const view = render(
    <div ref={frame}>
      {APP_TOUR.map((s) => (
        <div key={s.id} data-tour={s.anchor} />
      ))}
      <GuidedTour frame={frame} paused={false} modifierKey="Ctrl" />
    </div>,
  );
  return view;
}

/**
 * A declared box.
 *
 * happy-dom lays nothing out, so every rect is zero unless it is stated. That is
 * fine here: the ring's geometry is arithmetic on the anchor's rect, and the
 * arithmetic is the thing under test.
 */
function withRect(el: Element, box: { left: number; top: number; width: number; height: number }) {
  el.getBoundingClientRect = () => ({
    ...box,
    x: box.left,
    y: box.top,
    right: box.left + box.width,
    bottom: box.top + box.height,
    toJSON: () => box,
  });
}

/** A `ResizeObserver` that can be fired on demand; happy-dom has no layout to observe. */
class SpyResizeObserver {
  static last: SpyResizeObserver | null = null;
  observed: Element[] = [];
  private readonly ran: () => void;

  constructor(callback: () => void) {
    this.ran = callback;
    SpyResizeObserver.last = this;
  }

  observe(el: Element) {
    this.observed.push(el);
  }

  unobserve(el: Element) {
    this.observed = this.observed.filter((x) => x !== el);
  }

  disconnect() {
    this.observed = [];
  }

  fire() {
    this.ran();
  }
}

/** `step` 2 is the security report, whose ring pad is 4 and radius 16. */
const SECURITY = 2;

/** Mount the sequence on the security step with the anchors the test provides. */
function startOnSecurity() {
  useTour.setState({
    ready: true,
    showUnlock: false,
    showApp: true,
    replay: false,
    step: SECURITY,
  });
  useNavigation.setState({ surface: 'security' });
  const frame = createRef<HTMLDivElement>();
  const view = render(
    <div ref={frame}>
      <GuidedTour frame={frame} paused={false} modifierKey="Ctrl" />
    </div>,
  );
  const ring = view.container.querySelector('.gt-ring');
  if (!(ring instanceof HTMLElement)) throw new Error('no ring');
  return { frame, ring };
}

/** An anchor for the security step, with a stated box. */
function securityAnchor(box: { left: number; top: number; width: number; height: number }) {
  const el = document.createElement('div');
  el.setAttribute('data-tour', 'security');
  withRect(el, box);
  return el;
}

describe('GuidedTour', () => {
  beforeEach(() => {
    tourMarkSeen.mockReset();
    tourMarkSeen.mockResolvedValue(true);
    tourReset.mockReset();
    tourReset.mockResolvedValue(undefined);
  });

  afterEach(() => {
    // One test replaces the global ResizeObserver; undo it here rather than at the
    // end of that test, so a failure part-way through cannot leak it into the rest.
    vi.unstubAllGlobals();
    SpyResizeObserver.last = null;
  });

  it('opens on the first card, with the position in words', () => {
    start();

    expect(title()).toBe(APP_TOUR[0]?.title);
    // The eyebrow is the accessible progress indicator — the dot row is hidden.
    expect(screen.getByText('Step 1 of 4')).toBeInTheDocument();
    expect(screen.getByRole('dialog')).toHaveAccessibleName(APP_TOUR[0]?.title ?? '');
  });

  it('is not modal, so the app behind it stays available to a screen reader', () => {
    start();
    expect(screen.getByRole('dialog')).toHaveAttribute('aria-modal', 'false');
  });

  it('hides the dot row from assistive technology', () => {
    start();
    const dots = screen.getByRole('dialog').querySelector('.gt-dots');
    expect(dots).toHaveAttribute('aria-hidden', 'true');
  });

  it('keeps one card for the whole sequence rather than remounting it', async () => {
    const user = userEvent.setup();
    start();

    const before = screen.getByRole('dialog');
    await user.click(screen.getByRole('button', { name: 'Next' }));

    await waitFor(() => {
      expect(title()).toBe(APP_TOUR[1]?.title);
    });
    // The same node, moved. This is the whole reason the handoff rejected
    // dismiss-and-re-enter.
    expect(screen.getByRole('dialog')).toBe(before);
  });

  it('selects the view each step describes', async () => {
    const user = userEvent.setup();
    start();

    for (const step of APP_TOUR.slice(1)) {
      await user.click(screen.getByRole('button', { name: 'Next' }));
      await waitFor(() => {
        expect(title()).toBe(step.title);
      });
      expect(useNavigation.getState().surface).toBe(step.surface);
    }
  });

  it('offers Done on the last card and nothing after it', async () => {
    const user = userEvent.setup();
    start(APP_TOUR.length - 1);

    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Done' })).toBeInTheDocument();
    expect(tourMarkSeen).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Done' }));
    await waitFor(() => {
      expect(useTour.getState().showApp).toBe(false);
    });
    expect(tourMarkSeen).toHaveBeenCalledWith('app');
  });

  it('ends on Skip', async () => {
    const user = userEvent.setup();
    start();

    await user.click(screen.getByRole('button', { name: 'Skip' }));

    await waitFor(() => {
      expect(useTour.getState().showApp).toBe(false);
    });
    expect(tourMarkSeen).toHaveBeenCalledWith('app');
  });

  it('ends on the close button, rather than advancing', async () => {
    const user = userEvent.setup();
    start();

    await user.click(screen.getByRole('button', { name: 'End tour' }));

    await waitFor(() => {
      expect(useTour.getState().showApp).toBe(false);
    });
    // Not step 2: HO-002 has one exit, and the corner is one of its three doors.
    expect(tourMarkSeen).toHaveBeenCalledWith('app');
  });

  it('ends on Escape', async () => {
    const user = userEvent.setup();
    start(1);

    await user.keyboard('{Escape}');

    await waitFor(() => {
      expect(useTour.getState().showApp).toBe(false);
    });
    expect(tourMarkSeen).toHaveBeenCalledWith('app');
  });

  it('renders nothing while a modal is open, and keeps its place', () => {
    useTour.setState({ ready: true, showUnlock: false, showApp: true, replay: false, step: 2 });
    const frame = createRef<HTMLDivElement>();
    render(
      <div ref={frame}>
        <GuidedTour frame={frame} paused modifierKey="Ctrl" />
      </div>,
    );

    expect(screen.queryByRole('dialog')).toBeNull();
    expect(useTour.getState().step).toBe(2);
    expect(tourMarkSeen).not.toHaveBeenCalled();
  });

  it('names the platform modifier rather than a Mac glyph', () => {
    // COPY.md: "Do not ship a Mac glyph to a Windows build."
    start();
    const body = screen.getByRole('dialog').textContent;
    expect(body).toContain('CtrlC');
    expect(body).not.toContain('⌘');
  });

  it('moves focus to the primary action so the keyboard can drive it', async () => {
    start();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Next' })).toHaveFocus();
    });
  });

  /**
   * The two ways step 3's ring lost the second row of stat cards.
   *
   * The security pane renders an empty section while `security_report_run` is in
   * flight, so its anchor is not in the layout on the frame the placement runs —
   * measured at about 120ms of daylight between the two. Placement correctly
   * declines to point at nothing; what it used to do was never come back.
   */
  it('measures an anchor that only arrives after the step opens', async () => {
    const { frame, ring } = startOnSecurity();

    // Nothing to point at yet, so nothing is written: the ring keeps its place.
    await waitFor(() => {
      expect(ring.style.height).toBe('');
    });

    const late = securityAnchor({ left: 100, top: 200, width: 400, height: 300 });
    frame.current?.append(late);

    // 4px of pad on each side, from the step's own `ringPad`.
    await waitFor(() => {
      expect(ring.style.height).toBe('308px');
    });
    expect(ring.style.top).toBe('196px');
    expect(ring.style.left).toBe('96px');
    expect(ring.style.width).toBe('408px');
  });

  it('re-measures when the anchor grows a row under it', async () => {
    vi.stubGlobal('ResizeObserver', SpyResizeObserver);
    const { frame, ring } = startOnSecurity();
    const anchor = securityAnchor({ left: 100, top: 200, width: 400, height: 150 });
    frame.current?.append(anchor);

    await waitFor(() => {
      expect(ring.style.height).toBe('158px');
    });

    // Past the settle watch, so what follows can only be the ResizeObserver.
    await new Promise((resolve) => setTimeout(resolve, 420));

    const observer = SpyResizeObserver.last;
    if (observer === null) throw new Error('no ResizeObserver was constructed');
    expect(observer.observed).toContain(anchor);

    // A second row of stat cards: same anchor, twice the height.
    withRect(anchor, { left: 100, top: 200, width: 400, height: 300 });
    observer.fire();

    await waitFor(() => {
      expect(ring.style.height).toBe('308px');
    });
  });
});
