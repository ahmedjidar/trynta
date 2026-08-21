// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Tour state for the session.
 *
 * A store rather than props because the two halves of this feature are on
 * opposite sides of the lock gate — the lock-screen card is inside `LockScreen`,
 * the sequence is in the shell, and the replay action is three components deep
 * in Settings — and the persisted answer is one `tour_state` call that all three
 * need. Threading it would mean the shell passing a prop into a screen it only
 * renders while locked.
 *
 * ## What is authoritative, and what is not
 *
 * Rust owns *whether a tour has been seen*, including the debug-build replay
 * policy; this store never re-derives either. What it owns is the part with no
 * business being persisted: which card the sequence is on, and whether a card
 * has been dismissed in this session. Those reset when the app does, and should.
 *
 * ## Nothing here holds vault data
 *
 * Worth saying explicitly, because §4.9 requires the webview to be emptied on
 * lock and this store is deliberately *not* cleared there: it holds two booleans
 * and an integer, none of which describe the vault's contents. The theme store
 * is cleared on lock because a list of imported theme names is user data. A step
 * index is not.
 */

import { create } from 'zustand';

import { tourMarkSeen, tourReset, tourState } from '../../ipc';
import { APP_TOUR } from './content';

export interface TourStore {
  /** False until `load` has answered. Nothing renders before then. */
  ready: boolean;
  /** Show the master-password card on the lock screen. */
  showUnlock: boolean;
  /** Show the four-card sequence. */
  showApp: boolean;
  /** This build replays on every launch, so Settings says so. */
  replay: boolean;
  /** Which card the sequence is on. */
  step: number;

  /** Read the persisted state. Safe to call while locked. */
  load: () => Promise<void>;
  /** The close button, or Escape, on the lock-screen notice. */
  dismissUnlock: () => void;
  /**
   * Record that the lock-screen notice's moment has passed.
   *
   * Called after a successful unlock or create, whether or not it was dismissed:
   * the user is past it either way, and this is the first moment a vault file is
   * guaranteed to exist for the flag to be written to.
   */
  markUnlockSeen: () => void;
  /** Move to the next card. Next only — HO-002 has one exit, not four. */
  advance: () => void;
  /**
   * End the sequence.
   *
   * All three of HO-002's doors lead here: Skip, the close button and Escape. Its
   * `onEnd` reports which one it was and this does not, because nothing acts on
   * the difference — *"Someone who dismissed it has answered the question"*
   * (INTEGRATION.md §6).
   */
  finish: () => void;
  /**
   * Clear both flags and start the sequence again from the first card.
   *
   * INTEGRATION.md §6: *"Give people a way back… It costs one line and removes
   * the pressure to make the tour unskippable."*
   */
  restart: () => Promise<void>;
}

/**
 * A tour flag write is best-effort by design.
 *
 * The two ways it fails are "no vault file yet", which the command reports as
 * `false` rather than an error, and a storage failure. Neither is worth a toast:
 * the user dismissed an explanatory card, and the worst outcome of losing the
 * write is that they see it once more. Surfacing it would be the app complaining
 * about its own bookkeeping.
 */
function mark(which: 'unlock' | 'app'): void {
  void tourMarkSeen(which).catch(() => {
    /* see above */
  });
}

export const useTour = create<TourStore>((set, get) => ({
  ready: false,
  showUnlock: false,
  showApp: false,
  replay: false,
  step: 0,

  load: async () => {
    try {
      const state = await tourState();
      set({
        ready: true,
        showUnlock: state.showUnlock,
        showApp: state.showApp,
        replay: state.replay,
        step: 0,
      });
    } catch {
      // Fail towards *not* showing. A tour is the least important thing on
      // screen, and a first-run card that appears because a read failed would
      // appear on every launch for as long as the read kept failing.
      set({ ready: true, showUnlock: false, showApp: false });
    }
  },

  dismissUnlock: () => {
    set({ showUnlock: false });
    mark('unlock');
  },

  markUnlockSeen: () => {
    set({ showUnlock: false });
    mark('unlock');
  },

  advance: () => {
    const next = get().step + 1;
    if (next >= APP_TOUR.length) {
      get().finish();
      return;
    }
    set({ step: next });
  },

  finish: () => {
    set({ showApp: false, step: 0 });
    mark('app');
  },

  restart: async () => {
    await tourReset();
    // The sequence starts now, where the user is. The lock-screen card cannot —
    // it lives on a screen that is not on screen — so it returns at the next
    // lock, which is what the settings row promises.
    set({ showUnlock: true, showApp: true, step: 0 });
  },
}));
