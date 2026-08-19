// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Make an element behave like a title bar: press-and-move drags the window, and a
 * double-click maximises or restores it.
 *
 * Both are what a system titlebar does, and with `decorations: false` there is no system
 * titlebar left to do them.
 *
 * ## Why the target is checked
 *
 * The handler sits on the bar, but `mousedown` bubbles from every control inside it —
 * so without a check, pressing Lock or the close button would start dragging the window
 * instead of pressing the button. Only presses that land on an element which opted in
 * (`data-drag-region`) count, which is the same rule Tauri's own attribute uses and the
 * reason a nested button needs no opt-out of its own.
 */

import { useCallback } from 'react';
import type { MouseEvent } from 'react';

import { startDragging, toggleMaximizeWindow } from '../ipc';

/** Props to spread onto a title-bar element. */
export interface DragRegionProps {
  onMouseDown: (event: MouseEvent) => void;
  onDoubleClick: (event: MouseEvent) => void;
}

export function useDragRegion(): DragRegionProps {
  const onMouseDown = useCallback((event: MouseEvent) => {
    // Primary button only: a right-press opens the system menu, and a middle-press
    // should do nothing rather than fling the window across the desktop.
    if (event.button !== 0) return;
    if (!(event.target as HTMLElement).hasAttribute('data-drag-region')) return;
    // The press is being handed to the window manager, so the webview must not also
    // treat it as the start of a text selection.
    event.preventDefault();
    void startDragging();
  }, []);

  const onDoubleClick = useCallback((event: MouseEvent) => {
    if (!(event.target as HTMLElement).hasAttribute('data-drag-region')) return;
    void toggleMaximizeWindow();
  }, []);

  return { onMouseDown, onDoubleClick };
}
