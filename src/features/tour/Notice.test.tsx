// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The pre-unlock notice, and the two things it must not do.
 *
 * It must not take focus — the password field has it, and a notice that stole it
 * would make the first interaction of every first launch a click back into the
 * field the user was already in. And it must not mark the in-app sequence as
 * seen: two flags for two moments, and collapsing them would mean dismissing one
 * card silently skipped the other four.
 */

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { UNLOCK_NOTICE } from './content';
import { Notice } from './Notice';
import { useTour } from './store';

const tourMarkSeen = vi.hoisted(() => vi.fn());
const tourReset = vi.hoisted(() => vi.fn());
const tourState = vi.hoisted(() => vi.fn());
vi.mock('../../ipc', () => ({ tourMarkSeen, tourReset, tourState }));

/** The lock screen as it is on a first launch: a focused field, then the notice. */
function firstLaunch(showUnlock = true) {
  useTour.setState({ ready: true, showUnlock, showApp: true, replay: false, step: 0 });
  return render(
    <form>
      <input aria-label="Master password" type="password" autoFocus />
      <Notice />
    </form>,
  );
}

describe('Notice', () => {
  beforeEach(() => {
    tourMarkSeen.mockReset();
    tourMarkSeen.mockResolvedValue(true);
  });

  it('carries the eyebrow, the claim, the mechanism and the warning', () => {
    firstLaunch();

    expect(screen.getByText(UNLOCK_NOTICE.eyebrow)).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent(UNLOCK_NOTICE.title);
    expect(screen.getByText(UNLOCK_NOTICE.body)).toBeInTheDocument();
    expect(screen.getByText(UNLOCK_NOTICE.warning)).toBeInTheDocument();
  });

  it('names the product as the chrome names it', () => {
    // COPY.md calls a card that disagrees with the title bar "the single most
    // noticeable copy bug in this component".
    firstLaunch();
    expect(screen.getByRole('note').textContent).toContain('Trynta');
  });

  it('is a note in flow, not a dialog and not part of a sequence', () => {
    firstLaunch();

    expect(screen.getByRole('note')).toBeInTheDocument();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Next' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Skip' })).toBeNull();
  });

  it('leaves focus in the password field', () => {
    firstLaunch();
    expect(screen.getByLabelText('Master password')).toHaveFocus();
  });

  it('is dismissed by its close button, and records only the unlock flag', async () => {
    const user = userEvent.setup();
    firstLaunch();

    await user.click(screen.getByRole('button', { name: 'Dismiss' }));

    // The notice animates out first — 160ms on HO-002's accelerating curve, plus
    // its 10ms unmount margin — so it is still on screen for a moment after the
    // click. Asserting on the same tick would be asserting that the exit does
    // not exist.
    await waitFor(() => {
      expect(screen.queryByRole('note')).toBeNull();
    });
    expect(tourMarkSeen).toHaveBeenCalledWith('unlock');
    expect(tourMarkSeen).toHaveBeenCalledTimes(1);
    expect(useTour.getState().showApp).toBe(true);
  });

  it('is dismissed by Escape', async () => {
    const user = userEvent.setup();
    firstLaunch();

    await user.keyboard('{Escape}');

    await waitFor(() => {
      expect(screen.queryByRole('note')).toBeNull();
    });
    expect(tourMarkSeen).toHaveBeenCalledWith('unlock');
  });

  it('renders nothing once it has been seen', () => {
    firstLaunch(false);
    expect(screen.queryByRole('note')).toBeNull();
  });

  it('stays dismissed for the session even when the flag could not be written', async () => {
    // The fresh-install case: no vault file yet, so `tour_mark_seen` answers
    // `false`. The notice must still go away — it is marked again the moment the
    // vault is created.
    tourMarkSeen.mockResolvedValue(false);
    const user = userEvent.setup();
    firstLaunch();

    await user.click(screen.getByRole('button', { name: 'Dismiss' }));

    await waitFor(() => {
      expect(screen.queryByRole('note')).toBeNull();
    });
    expect(useTour.getState().showUnlock).toBe(false);
  });

  it('does not surface a failed write as an error', async () => {
    tourMarkSeen.mockRejectedValue(new Error('storage'));
    const user = userEvent.setup();
    firstLaunch();

    await user.click(screen.getByRole('button', { name: 'Dismiss' }));

    await waitFor(() => {
      expect(screen.queryByRole('note')).toBeNull();
    });
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
