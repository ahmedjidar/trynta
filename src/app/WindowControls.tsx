/**
 * Minimise, maximise and close, drawn in the app's own vocabulary.
 *
 * Windows only. macOS keeps its native traffic lights, which the OS draws over our
 * content because `tauri.macos.conf.json` sets `titleBarStyle: Overlay` — a platform
 * convention, not chrome, and one users reach for by muscle memory in a fixed place.
 *
 * Close is the only control that gets a colour on hover, and it gets the danger token
 * rather than a red of its own. Every other state is the same hover fill the rest of
 * the title bar uses, so the three buttons read as part of the app instead of as a
 * borrowed widget.
 */

import { useEffect, useState } from 'react';

import { Glyph } from '../components/Glyph';
import { cn } from '../lib/cn';
import {
  closeWindow,
  isWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from '../ipc';

/** Shared geometry: a 30px square with the title bar's own radius step. */
const BUTTON =
  'flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-md ' +
  'text-text-secondary duration-fast transition-[background-color,color,transform] ' +
  'hover:text-text-primary active:scale-[.92]';

export function WindowControls() {
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
    // No drag marker on these: `useDragRegion` only starts a drag when the press lands
    // on an element that carries `data-drag-region`, so a button inside the bar needs no
    // opt-out of its own.
    <div className="flex shrink-0 items-center gap-1">
      <button
        type="button"
        data-focus-ring
        aria-label="Minimise"
        title="Minimise"
        onClick={() => {
          void minimizeWindow();
        }}
        className={cn(BUTTON, 'hover:bg-surface-hover')}
      >
        <Glyph name="windowMinimise" size={14} />
      </button>

      <button
        type="button"
        data-focus-ring
        aria-label={maximized ? 'Restore' : 'Maximise'}
        title={maximized ? 'Restore' : 'Maximise'}
        onClick={() => {
          void toggleMaximizeWindow();
        }}
        className={cn(BUTTON, 'hover:bg-surface-hover')}
      >
        <Glyph name={maximized ? 'windowRestore' : 'windowMaximise'} size={12} />
      </button>

      <button
        type="button"
        data-focus-ring
        aria-label="Close"
        title="Close"
        onClick={() => {
          void closeWindow();
        }}
        className={cn(BUTTON, 'hover:bg-status-danger hover:text-text-on-accent')}
      >
        <Glyph name="windowClose" size={14} />
      </button>
    </div>
  );
}
