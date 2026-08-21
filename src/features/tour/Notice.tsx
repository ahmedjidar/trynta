// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The pre-unlock notice — HO-002's `GuidedTour.notice`, as a component.
 *
 * Same DOM, same class names, same `role="note"`, same `data-side`. The styling
 * is `guided-tour.css`, vendored verbatim; nothing here carries a value.
 *
 * README.md: *"It is a normal block element in normal flow — put it directly
 * after the field it describes, with `margin-top: 11px` so the beak reaches the
 * field."* So it is not positioned and it is not part of the sequence; the beak
 * is centred by CSS rather than computed, and `side: 'bottom'` means the notice
 * sits below its subject with the beak on its top edge.
 *
 * ## It does not take focus
 *
 * `LockScreen` puts the caret in the password field on mount. Escape dismisses
 * the notice through a window listener, and Tab reaches its close button in
 * document order because the notice follows the form's controls. Stealing focus
 * would make the first interaction of every first launch a click back into the
 * field the user was already in.
 *
 * ## The warning slot
 *
 * COPY.md permits it only for something genuinely irreversible, in `--warn`
 * rather than `--danger`: nothing has gone wrong, and an unrecoverable master
 * password is how this product works.
 */

import { useEffect, useId, useRef, useState } from 'react';

import { cn } from '../../lib/cn';
import { UNLOCK_NOTICE } from './content';
import { useTour } from './store';

/** HO-002's exit duration plus its 10ms unmount margin, and the reduced pair. */
const OUT_MS = 170;
const OUT_MS_REDUCED = 110;

function prefersReducedMotion(): boolean {
  return (
    typeof globalThis.matchMedia === 'function' &&
    globalThis.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export function Notice() {
  const show = useTour((s) => s.showUnlock);
  const dismissUnlock = useTour((s) => s.dismissUnlock);
  const [exiting, setExiting] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const titleId = useId();

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  useEffect(() => {
    if (!show) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      // Not `preventDefault`: on the lock screen Escape does nothing else, and
      // swallowing it would be the notice taking a key from a screen it is a
      // guest on.
      start();
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
    // `start` is stable for the life of the mount.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [show]);

  function start() {
    if (timer.current !== null) return;
    setExiting(true);
    timer.current = setTimeout(dismissUnlock, prefersReducedMotion() ? OUT_MS_REDUCED : OUT_MS);
  }

  if (!show) return null;

  return (
    <div
      className={cn('gt-notice', exiting && 'gt-notice--exit')}
      // Both are HO-002's: a note rather than a dialog, because it is page
      // furniture in normal flow, and `data-side` for which edge the beak is on.
      role="note"
      data-side="bottom"
      aria-labelledby={titleId}
    >
      <div className="gt-beak" />
      <div className="gt-notice-inner">
        <div className="gt-head">
          <span className="gt-eyebrow">{UNLOCK_NOTICE.eyebrow}</span>
          <button type="button" className="gt-close" aria-label="Dismiss" onClick={start}>
            <XMark />
          </button>
        </div>
        <h2 className="gt-title" id={titleId}>
          {UNLOCK_NOTICE.title}
        </h2>
        <p className="gt-body">{UNLOCK_NOTICE.body}</p>
        <div className="gt-warn">
          <span className="gt-warn-icon">
            <WarnMark />
          </span>
          <span className="gt-warn-text">{UNLOCK_NOTICE.warning}</span>
        </div>
      </div>
    </div>
  );
}

/**
 * HO-002's close glyph.
 *
 * Inline rather than through `components/Glyph`: the handoff ships its own two
 * marks at its own stroke weights, and `Glyph` is the Lucide set at the design's
 * `--icon-stroke`. Substituting one for the other would be a visual edit.
 */
export function XMark() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.9}
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M18 6 6 18M6 6l12 12" />
    </svg>
  );
}

/** HO-002's warning mark. Outline circle, stem, dot. */
function WarnMark() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 8v4.5" strokeLinecap="round" />
      <circle cx="12" cy="16.2" r=".9" fill="currentColor" stroke="none" />
    </svg>
  );
}
