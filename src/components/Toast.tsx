/**
 * Toast — components.md §15.
 *
 * Closes the handoff's gap 5: *"Toast is not announced to assistive tech."* It is a
 * `role="status"` with `aria-live="polite"`, so a copy is announced without stealing
 * focus. That matters more here than in most apps: the whole point of copying a
 * password in Rust is that the user never sees it, so the confirmation is the only
 * feedback there is.
 *
 * §15: auto-dismisses after 2.2s, and *"a new message resets the timer rather than
 * stacking"*. One toast, one timer.
 *
 * The suffix is not decoration. §15 shows "· clipboard clears in 30s" and that is a
 * security-relevant promise, so it renders the *configured* interval rather than a
 * hardcoded 30 — and it is omitted entirely when clipboard clearing is off, because
 * claiming a clear that will not happen is worse than saying nothing.
 */

import { useEffect } from 'react';

import { Glyph } from './Glyph';

/** §15: 2.2 seconds. */
const DISMISS_MS = 2200;

export interface ToastProps {
  /** The message, or `null` for nothing showing. */
  message: string | null;
  /** Called when the timer expires. */
  onDismiss: () => void;
  /** Seconds until the clipboard is cleared, or `null` if clearing is off. */
  clipboardSeconds: number | null;
}

export function Toast({ message, onDismiss, clipboardSeconds }: ToastProps) {
  useEffect(() => {
    if (message === null) return undefined;
    // Keyed on the message, so a new one restarts the timer rather than stacking.
    const timer = setTimeout(onDismiss, DISMISS_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [message, onDismiss]);

  return (
    // Rendered even when empty: an `aria-live` region has to exist in the DOM before
    // the text arrives, or the first announcement is missed.
    <div className="toast-region" role="status" aria-live="polite">
      {message === null ? null : (
        <div className="toast">
          <Glyph name="check" />
          <span>{message}</span>
          {clipboardSeconds === null ? null : (
            <span className="toast__suffix">· clipboard clears in {clipboardSeconds}s</span>
          )}
        </div>
      )}
    </div>
  );
}
