/**
 * Toast — HO-002 `overlays/Toast.tsx`, components.md §15.
 *
 * `role="status"` with `aria-live="polite"`, so a copy is announced without stealing
 * focus. That matters more here than in most apps: the whole point of copying a password
 * in Rust is that the user never sees it, so the confirmation is the only feedback there
 * is. HO-002 upgraded this from the HTML original's silent div; the live region is kept
 * mounted when empty, because an `aria-live` region has to exist before the text arrives
 * or the first announcement is missed.
 *
 * Auto-dismisses after 2.2s, and a new message resets the timer rather than stacking.
 *
 * The suffix is not decoration. HO-002 hardcodes "· clipboard clears in 30s"; that is a
 * security-relevant promise, so it renders the *configured* interval and is omitted
 * entirely when clipboard clearing is off — claiming a clear that will not happen is worse
 * than saying nothing.
 */

import { useEffect } from 'react';
import { Glyph } from './Glyph';

export interface ToastProps {
  /** The message, or `null` for nothing showing. */
  message: string | null;
  /** Called when the timer expires. */
  onDismiss: () => void;
  /** Seconds until the clipboard is cleared, or `null` if clearing is off. */
  clipboardSeconds: number | null;
}

/** HO-002 and components.md §15 agree: 2.2 seconds. */
const DISMISS_MS = 2200;

export function Toast({ message, onDismiss, clipboardSeconds }: ToastProps) {
  useEffect(() => {
    if (message === null) return undefined;
    const timer = setTimeout(onDismiss, DISMISS_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [message, onDismiss]);

  return (
    <div role="status" aria-live="polite">
      {message === null ? null : (
        <div className="animate-toast-in bg-text-primary text-control text-text-inverse shadow-window absolute bottom-6 left-1/2 z-[5] flex h-9 -translate-x-1/2 items-center gap-2 rounded-full px-[18px] font-semibold">
          <span className="text-accent-hover">
            <Glyph name="check" />
          </span>
          {message}
          {clipboardSeconds === null ? null : (
            <span className="opacity-55">· clipboard clears in {clipboardSeconds}s</span>
          )}
        </div>
      )}
    </div>
  );
}
