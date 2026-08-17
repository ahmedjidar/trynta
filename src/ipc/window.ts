/**
 * Revealing the window after the first paint.
 *
 * `tauri.conf.json` creates the main window with `"visible": false`, and that is
 * deliberate: a window shown before the theme is resolved flashes the wrong palette
 * on every launch, which is exactly what `visible: false` exists to prevent.
 *
 * **Nothing was calling `show()`.** The app compiled, passed every check, built a
 * bundle, and opened no window at all — the only process with a title was the
 * single-instance plugin's hidden helper. Static checks cannot catch this: there is
 * no missing symbol and no failing type. It took launching the real WebView2 to see
 * it, which is why that is worth doing before building five more panes on top.
 *
 * Called once the theme has been applied, so the first frame the user sees is the
 * right one.
 *
 * Lives in `src/ipc/` rather than `src/app/` because that is where the eslint rule
 * funnels every Tauri API. The boundary is the point, and it caught this on the first
 * lint after the fix.
 */

import { getCurrentWindow } from '@tauri-apps/api/window';

/** Whether the window has already been revealed, so a re-render cannot re-show it. */
let revealed = false;

/**
 * Show and focus the main window.
 *
 * Safe to call more than once and safe to call outside Tauri — in a browser or a
 * test the import resolves but the IPC call fails, and a failure here must never
 * take down the app: the worst case is a window the user has to click, which beats a
 * blank screen with an unhandled rejection behind it.
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
