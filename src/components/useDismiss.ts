/**
 * Hold a surface on screen long enough to animate out.
 *
 * React unmounts on the frame the parent stops rendering a component, so a sheet with
 * an entry animation and no exit appears smoothly and then vanishes — which reads as a
 * crash rather than as a dismissal. This delays the parent's callback by the length of
 * the exit animation and reports `closing` in the meantime, so the surface can swap its
 * entry animation for its exit one.
 *
 * The delay is a timer rather than an `animationend` listener on purpose: under
 * `prefers-reduced-motion` the animation collapses to 0.01ms and may not fire an event
 * at all, and a dialog that will not close is a worse failure than one that closes
 * fractionally early.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

/** Matches `--duration-fast`. Long enough to read as movement, short enough not to wait. */
const EXIT_MS = 140;

export interface Dismissal {
  /** True once dismissal has started. Swap the entry animation for the exit one. */
  closing: boolean;
  /** Start the exit. Calling it twice does nothing the second time. */
  dismiss: () => void;
}

/**
 * @param onDone - The parent's real close callback, fired after the exit animation.
 * @param ms - Exit duration. Defaults to the design's `--duration-fast`.
 */
export function useDismiss(onDone: () => void, ms: number = EXIT_MS): Dismissal {
  const [closing, setClosing] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );

  const dismiss = useCallback(() => {
    if (timer.current !== null) return;
    setClosing(true);
    timer.current = setTimeout(onDone, ms);
  }, [onDone, ms]);

  return { closing, dismiss };
}
