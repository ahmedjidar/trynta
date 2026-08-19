// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The window itself: revealing it, and the controls that replace the OS titlebar.
 *
 * Lives in `src/ipc/` rather than `src/app/` because that is where the eslint rule
 * funnels every Tauri API. The boundary is the point.
 *
 * ## Why the app draws its own controls
 *
 * `tauri.conf.json` sets `decorations: false` on Windows, so there is no system
 * titlebar to theme — the alternative was a grey OS bar bolted onto a rounded, dark
 * window, which is the one part of the frame a stylesheet cannot reach. macOS keeps
 * its native traffic lights (`tauri.macos.conf.json` sets `titleBarStyle: Overlay`),
 * because those are a platform convention rather than chrome, and the OS positions
 * them over our content correctly.
 *
 * Every function here is safe to call outside Tauri — in a browser or a unit test the
 * import resolves and the IPC call fails, and a window control that throws must never
 * take down the app.
 */

import { getCurrentWindow } from '@tauri-apps/api/window';

/** Whether the window has already been revealed, so a re-render cannot re-show it. */
let revealed = false;

/**
 * Show and focus the main window.
 *
 * `tauri.conf.json` creates it with `"visible": false`, deliberately: a window shown
 * before the theme is resolved flashes the wrong palette on every launch.
 *
 * **Nothing was calling `show()` once.** The app compiled, passed every check, built a
 * bundle, and opened no window at all. Static checks cannot catch that: there is no
 * missing symbol and no failing type.
 *
 * Safe to call more than once. The worst case on failure is a window the user has to
 * click, which beats a blank screen with an unhandled rejection behind it.
 */
export async function revealWindow(): Promise<void> {
  if (revealed) return;
  revealed = true;
  try {
    const window = getCurrentWindow();
    await window.show();
    await window.setFocus();
  } catch {
    // Not running under Tauri, or the capability is missing. Either way there is
    // nothing useful to tell the user and nothing to retry.
  }
}

/**
 * Start moving the window with the pointer.
 *
 * Called from a `mousedown` on the title bar rather than left to Tauri's
 * `data-tauri-drag-region` attribute. That attribute is handled by a listener in
 * Tauri's injected init script, and this build does not get it — `__TAURI_INTERNALS__`
 * arrives carrying `plugins` and nothing else, so the attribute is inert and the title
 * bar does not move the window. Measured, not assumed: a synthetic press-and-drag on
 * the bar left `GetWindowRect` unchanged.
 *
 * Calling the command directly is the documented alternative and has one fewer moving
 * part: the same capability (`core:window:allow-start-dragging`) authorises it, and the
 * decision about *which* presses count as a drag stays in our own handler.
 */
export async function startDragging(): Promise<void> {
  try {
    await getCurrentWindow().startDragging();
  } catch {
    // Not under Tauri.
  }
}

/** Minimise the window. */
export async function minimizeWindow(): Promise<void> {
  try {
    await getCurrentWindow().minimize();
  } catch {
    // Not under Tauri.
  }
}

/**
 * Maximise, or restore if already maximised.
 *
 * One command rather than two so the button cannot disagree with the window about
 * which state it is in.
 */
export async function toggleMaximizeWindow(): Promise<void> {
  try {
    await getCurrentWindow().toggleMaximize();
  } catch {
    // Not under Tauri.
  }
}

/** Close the window, which ends the process. */
export async function closeWindow(): Promise<void> {
  try {
    await getCurrentWindow().close();
  } catch {
    // Not under Tauri.
  }
}

/** Whether the window is maximised, for the restore/maximise glyph. */
export async function isWindowMaximized(): Promise<boolean> {
  try {
    return await getCurrentWindow().isMaximized();
  } catch {
    return false;
  }
}

/**
 * Call `listener` whenever the window is resized, and once immediately.
 *
 * Used for two things: swapping the maximise glyph for a restore glyph, and squaring
 * off the window's rounded corners when it is maximised — a rounded corner against a
 * screen edge shows the desktop through it.
 *
 * @returns An unsubscribe function. Resolves to a no-op outside Tauri.
 */
export async function onWindowResized(listener: () => void): Promise<() => void> {
  try {
    const unlisten = await getCurrentWindow().onResized(() => {
      listener();
    });
    return unlisten;
  } catch {
    return () => {
      // Nothing was subscribed.
    };
  }
}
