// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Confirmation toast — components.md §15.
 *
 * `role="status"` with `aria-live="polite"`, so a copy is announced without stealing
 * focus. That matters more here than in most apps: the whole point of copying a password
 * in Rust is that the user never sees it, so the confirmation is the only feedback there
 * is. The live region stays mounted when empty, because an `aria-live` region has to
 * exist before the text arrives or the first announcement is missed.
 *
 * Auto-dismisses after 2.2s, and a new message resets the timer rather than stacking.
 *
 * ## In and out
 *
 * The pill is a **transition on a node that stays mounted**, not a pair of animations
 * on one that comes and goes. React unmounts on the frame the message clears, so an
 * exit animation on a conditionally rendered node never gets to play — the element is
 * gone before its first keyframe. Keeping the node and toggling `data-visible` lets the
 * same two properties carry it both ways, with no timer to keep in sync and nothing to
 * unmount mid-flight.
 *
 * The text of the last message is held after the message clears so there is something to
 * read while it leaves.
 *
 * The suffix is not decoration. The design hardcodes "· clipboard clears in 30s"; that is
 * a security-relevant promise, so it renders the *configured* interval and is omitted
 * entirely when clipboard clearing is off — claiming a clear that will not happen is
 * worse than saying nothing.
 */

import { useEffect, useState } from 'react';

import { Glyph } from './Glyph';

export interface ToastProps {
  /** The message, or `null` for nothing showing. */
  message: string | null;
  /** Called when the timer expires. */
  onDismiss: () => void;
  /** Seconds until the clipboard is cleared, or `null` if clearing is off. */
  clipboardSeconds: number | null;
}

/** components.md §15. */
const DISMISS_MS = 2200;

export function Toast({ message, onDismiss, clipboardSeconds }: ToastProps) {
  // The last non-null message, so the pill has text to show on the way out. Adjusted
  // during render rather than in an effect — React's own pattern for deriving state
  // from a prop, and the only one that has the new text ready on the same frame the
  // pill starts moving.
  const [held, setHeld] = useState<string | null>(message);
  if (message !== null && message !== held) setHeld(message);
  const text = message ?? held;

  useEffect(() => {
    if (message === null) return undefined;
    const timer = setTimeout(onDismiss, DISMISS_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [message, onDismiss]);

  return (
    // The live region is a full-width centring track, and the pill inside it owns no
    // horizontal transform at all.
    //
    // It used to be `left-1/2 -translate-x-1/2` on the pill itself while the entry
    // keyframe animated `translate(-50%, 12px) → translate(-50%, 0)`. Two rules writing
    // the same `transform` property is one rule: for the 320 ms of the animation the
    // keyframe won, and on its last frame the utility took over — so the toast appeared
    // left of centre and then jumped. Centring with flex leaves `transform` to the
    // transition alone, which is what makes it rise straight up.
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none absolute inset-x-0 bottom-6 z-[5] flex justify-center"
    >
      {text === null ? null : (
        <div
          className="toast bg-text-primary text-control text-text-inverse shadow-window flex h-9 max-w-[min(90%,var(--measure-pane))] items-center gap-2 rounded-full px-[18px] font-semibold"
          data-visible={message === null ? undefined : ''}
        >
          <span className="text-accent-hover shrink-0">
            <Glyph name="check" />
          </span>
          <span className="truncate">{text}</span>
          {clipboardSeconds === null ? null : (
            <span className="shrink-0 opacity-55">· clipboard clears in {clipboardSeconds}s</span>
          )}
        </div>
      )}
    </div>
  );
}
