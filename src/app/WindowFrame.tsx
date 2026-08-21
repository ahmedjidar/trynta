// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The window's own surface: the app's background, and the corner behaviour that goes
 * with a frameless window.
 *
 * `decorations: false` removes the system titlebar, which is the whole point — a grey
 * OS bar bolted to the top of a dark themed window is the one piece of chrome a
 * stylesheet cannot reach, and it was the first thing that looked wrong.
 *
 * ## Why the corners are the platform's and not the token layer's
 *
 * `--radius-window` is 20px, and a 20px radius here does nothing: measured on WebView2,
 * the pixels outside a CSS radius on the root element are still painted, so the visible
 * corner is whatever the compositor draws. Windows 11 rounds every top-level window at
 * its own radius through DWM, and it does so whether the window is transparent or not —
 * `transparent: true` was tried and produced an identical corner while costing subpixel
 * text antialiasing, which is a bad trade in an app whose smallest type is 11px.
 *
 * Forcing 20px would mean `SetWindowRgn`, which clips without antialiasing and gives a
 * visibly jagged curve. So the window wears the platform's corner — the same one every
 * native Windows 11 app has — and the design's radius scale is spent where it shows: the
 * panes, cards, rows and controls inside.
 *
 * ## Maximised
 *
 * DWM squares the corners itself when a window is maximised. The hairline goes with
 * them: an inset ring against a screen edge reads as a stray line.
 */

import { useEffect, useState } from 'react';
import type { ReactNode, Ref } from 'react';

import { cn } from '../lib/cn';
import { isWindowMaximized, onWindowResized } from '../ipc';

export interface WindowFrameProps {
  /** The window's contents. Absent for the one frame before the vault state is known. */
  children?: ReactNode;
  /**
   * The frame element, for the guided tour's positioning context.
   *
   * HO-002 INTEGRATION.md §2: the tour is bounded by the app's chrome, not the
   * viewport, and its frame must be `position: relative` — which this already is,
   * along with the `overflow: hidden` the handoff calls recommended.
   */
  frameRef?: Ref<HTMLDivElement> | undefined;
}

export function WindowFrame({ children, frameRef }: WindowFrameProps) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let live = true;
    let unlisten: (() => void) | undefined;

    const sync = () => {
      void isWindowMaximized().then((next) => {
        if (live) setMaximized(next);
      });
    };

    sync();
    void onWindowResized(sync).then((off) => {
      if (live) unlisten = off;
      else off();
    });

    return () => {
      live = false;
      unlisten?.();
    };
  }, []);

  return (
    <div
      ref={frameRef}
      data-maximized={maximized ? '' : undefined}
      className={cn(
        'bg-surface-app text-text-primary relative flex h-full w-full flex-col overflow-hidden',
        // An inset hairline reads as the window's edge while it floats, and as a stray
        // line once it is flush against the screen.
        maximized ? null : 'shadow-hairline',
      )}
    >
      {children}
    </div>
  );
}
