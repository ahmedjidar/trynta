/**
 * Interface scale — `Ctrl`/`Cmd` with `+`, `-` and `0`.
 *
 * ## Why CSS `zoom` and not a transform
 *
 * `transform: scale()` rasterises: it scales the painted result, so text at 1.25 is a
 * blown-up 1.0 bitmap and every hairline turns fuzzy. CSS `zoom` is a *layout* property
 * in Chromium — the engine relays out at the new scale and renders text at its real
 * size, so a 13px row at 1.25 is a genuine 16.25px row with crisp glyphs and a hairline
 * that is still one device pixel.
 *
 * It is written through the CSSOM rather than as a markup attribute, which is the
 * escape hatch SPEC-V1 §7.6 sanctions: `style-src 'self'` governs inline stylesheets
 * and markup `style=""`, not a script setting a property on an element.
 *
 * ## Why a ladder rather than a percentage
 *
 * Every step has to land the fixed row heights on whole pixels — the design's spatial
 * system is 4pt with 0.5px hairlines, and an arbitrary 1.07 puts a 60px row at 64.2px
 * and softens every edge in the product. These seven steps keep the common heights
 * (24, 30, 32, 40, 52, 60) integral.
 *
 * ## Not persisted
 *
 * SPEC-V1 §4.5's plaintext key list is exhaustive and has no entry for this, and the
 * encrypted settings blob is unreadable at the moment the shell first paints. So the
 * level is session-scoped and resets to {@link DEFAULT_ZOOM} on launch. Giving it a
 * home is a spec question, recorded in `handoffs/MANIFEST.md`.
 */

/** The ladder, ascending. Every step keeps the design's row heights on whole pixels. */
const LEVELS = [0.8, 0.9, 1, 1.1, 1.25, 1.4, 1.6] as const;

/**
 * Where the app starts.
 *
 * Not 1.0: the design's type scale tops out at 13px for body text, which is drawn for a
 * 1360-wide window seen at arm's length on a laptop panel. On the 1440-wide default and
 * anything above it, 1.0 leaves the panes short of the space they have and the text
 * smaller than it should be.
 */
export const DEFAULT_ZOOM = 1.1;

/** Current level. Module-scoped so the shell and the shortcut agree without a store. */
let current: number = DEFAULT_ZOOM;

/** Read the current level, for the settings row that displays it. */
export function zoomLevel(): number {
  return current;
}

/**
 * Apply a level, clamped to the ladder's range.
 *
 * @returns The level actually applied, which may differ from the request.
 */
export function applyZoom(level: number): number {
  const min = LEVELS[0];
  const max = LEVELS[LEVELS.length - 1] ?? min;
  current = Math.min(max, Math.max(min, level));
  // `zoom` on the root scales the whole document including the window frame, so the
  // border radius and the hairline scale with everything else rather than staying
  // pinned at one size while the content grows past them.
  document.documentElement.style.setProperty('zoom', String(current));
  return current;
}

/** Step to the next level up or down the ladder. */
function step(direction: 1 | -1): number {
  // Nearest rung first, so a level restored from outside the ladder still moves one
  // step rather than jumping to an end.
  let nearest = 0;
  for (let i = 1; i < LEVELS.length; i += 1) {
    const a = LEVELS[i] ?? 1;
    const b = LEVELS[nearest] ?? 1;
    if (Math.abs(a - current) < Math.abs(b - current)) nearest = i;
  }
  const next = Math.min(LEVELS.length - 1, Math.max(0, nearest + direction));
  return applyZoom(LEVELS[next] ?? DEFAULT_ZOOM);
}

/** One step larger. */
export const zoomIn = (): number => step(1);

/** One step smaller. */
export const zoomOut = (): number => step(-1);

/** Back to {@link DEFAULT_ZOOM}. */
export const zoomReset = (): number => applyZoom(DEFAULT_ZOOM);

/**
 * Bind the shortcuts and apply the starting level.
 *
 * `metaKey || ctrlKey` rather than a platform branch: on Windows only Ctrl is pressed
 * and on macOS only Command is, so accepting both is the same behaviour on each
 * platform without a `#cfg`-by-another-name in the frontend — and it is what SPEC-V1
 * §8 means by resolving the modifier rather than hardcoding it.
 *
 * Both spellings of each key are accepted because the layout decides which arrives:
 * `+` needs Shift on most layouts and reports as `+`, while the unshifted key reports
 * as `=`. `NumpadAdd`/`NumpadSubtract` come through `event.code`.
 *
 * @param onChange - Called with the new level after any change, for the settings row.
 * @returns An unsubscribe function.
 */
export function bindZoomShortcuts(onChange?: (level: number) => void): () => void {
  applyZoom(current);

  const onKey = (event: KeyboardEvent) => {
    if (!(event.metaKey || event.ctrlKey)) return;

    const key = event.key;
    const code = event.code;
    let next: number | null = null;

    if (key === '+' || key === '=' || code === 'NumpadAdd') next = zoomIn();
    else if (key === '-' || key === '_' || code === 'NumpadSubtract') next = zoomOut();
    else if (key === '0' || code === 'Numpad0') next = zoomReset();

    if (next === null) return;
    event.preventDefault();
    onChange?.(next);
  };

  const onWheel = (event: WheelEvent) => {
    if (!(event.metaKey || event.ctrlKey)) return;
    event.preventDefault();
    onChange?.(event.deltaY < 0 ? zoomIn() : zoomOut());
  };

  globalThis.addEventListener('keydown', onKey);
  // Not passive: the whole point is to stop the webview's own zoom from also firing.
  globalThis.addEventListener('wheel', onWheel, { passive: false });

  return () => {
    globalThis.removeEventListener('keydown', onKey);
    globalThis.removeEventListener('wheel', onWheel);
  };
}
