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
import { beforeEach, describe, expect, it, vi } from 'vitest';
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

describe('GuidedTour', () => {
  beforeEach(() => {
    tourMarkSeen.mockReset();
    tourMarkSeen.mockResolvedValue(true);
    tourReset.mockReset();
    tourReset.mockResolvedValue(undefined);
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
});
