/**
 * Title bar: identity, the search trigger, appearance and lock.
 *
 * ## Three departures from the design, each forced
 *
 * **Window controls, not traffic lights.** The design is drawn inside a picture of a
 * Mac, and those three dots belong to the picture. On Windows there is no system
 * titlebar at all (`decorations: false`), so the app draws minimise / maximise / close
 * itself, in the app's own vocabulary. On macOS the OS floats its real traffic lights
 * over the content and the bar just leaves room for them.
 *
 * **The modifier is resolved, not typed.** The design prints `⌘K`. SPEC-V1 §8 forbids
 * hardcoding a modifier; it comes from `app_platform_info`, so this reads `CtrlK` on
 * Windows.
 *
 * **The theme control is tri-state.** The design toggles dark/light. CLAUDE.md §3 requires
 * `system` following the OS, so the button cycles three ways and names the *stored* mode —
 * a control labelled "Dark" while following a dark OS would be lying about what is stored.
 */

import { cn } from '../lib/cn';
import { Glyph } from '../components/Glyph';
import type { GlyphName } from '../components/Glyph';
import { WindowControls } from './WindowControls';
import { useDragRegion } from './useDragRegion';
import { useThemeStore } from '../theme/store';
import type { ThemeMode } from '../theme/mode';

export interface TitleBarProps {
  /** Opens the command palette. */
  onOpenPalette: () => void;
  /** Locks the vault. */
  onLock: () => void;
  /** The platform's modifier label, from `app_platform_info`. Never hardcoded. */
  modifierKey: string;
  /** Which OS, so the window controls land on the side that platform puts them. */
  os: string;
}

/** Cycle order: what the next press gives you. */
const NEXT: Record<ThemeMode, ThemeMode> = { dark: 'light', light: 'system', system: 'dark' };

const LABEL: Record<ThemeMode, string> = { dark: 'Dark', light: 'Light', system: 'System' };

const MODE_GLYPH: Record<ThemeMode, GlyphName> = {
  dark: 'themeDark',
  light: 'themeLight',
  system: 'themeSystem',
};

function ToolbarButton({
  onClick,
  title,
  children,
}: {
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      data-focus-ring
      className={cn(
        'border-hairline bg-surface-panel flex h-[30px] shrink-0 items-center gap-1.5 rounded-full border px-3',
        'text-caption text-text-secondary shadow-inner-top font-semibold',
        'duration-base transition-[background-color,color,transform]',
        'hover:bg-surface-raised hover:text-text-primary active:scale-[.96]',
      )}
    >
      {children}
    </button>
  );
}

export function TitleBar({ onOpenPalette, onLock, modifierKey, os }: TitleBarProps) {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const drag = useDragRegion();

  // macOS draws its own traffic lights over our content, on the left. Windows has no
  // system chrome at all here, so the app supplies the three controls on the right.
  const macOS = os === 'macos';

  return (
    // The bar is the drag region: `useDragRegion` starts a window move only when the
    // press lands on an element carrying `data-drag-region`, so the controls inside it
    // keep their clicks without needing an opt-out.
    <div
      data-drag-region
      {...drag}
      className={cn(
        'border-hairline bg-surface-chrome vibrancy relative z-[3] flex h-[52px] shrink-0 items-center border-b pr-3',
        // Room for the traffic lights, which the OS positions at a fixed inset.
        macOS ? 'pl-[var(--pad-traffic-lights)]' : 'pl-4',
      )}
    >
      <div
        data-drag-region
        className={cn(
          'text-body pointer-events-none flex shrink-0 items-center gap-2 font-bold tracking-tight',
          macOS && 'ml-[68px]',
        )}
      >
        <span className="bg-accent text-badge-sm text-text-on-accent shadow-accent-glow flex h-[18px] w-[18px] items-center justify-center rounded-xs font-extrabold">
          K
        </span>
        Keyring
      </div>

      {/* Also a drag region: this wrapper spans most of the bar, and only the element
          under the pointer counts — so without it the middle two-thirds of the title bar
          would be dead to dragging. The search button inside is its own target and
          still receives its click. */}
      <div data-drag-region className="flex flex-1 justify-center px-5">
        <button
          type="button"
          data-focus-ring
          onClick={onOpenPalette}
          className={cn(
            'border-hairline bg-surface-panel flex h-8 w-[380px] cursor-text items-center gap-2 rounded-full border px-3',
            'text-caption text-text-muted shadow-inner-top',
            'duration-moderate transition-[box-shadow,border-color]',
            'hover:border-strong hover:shadow-search-hover',
          )}
        >
          <Glyph name="search" size={12} />
          Search vault, actions, tags
          <span className="border-strong text-micro ml-auto rounded-xs border px-[5px] leading-4 font-semibold">
            {modifierKey}K
          </span>
        </button>
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <ToolbarButton
          onClick={() => {
            void setMode(NEXT[mode]);
          }}
          title={`Appearance: ${LABEL[mode]}. Click for ${LABEL[NEXT[mode]]}.`}
        >
          <Glyph name={MODE_GLYPH[mode]} />
          {LABEL[mode]}
        </ToolbarButton>
        <ToolbarButton onClick={onLock} title={`Lock vault (${modifierKey}L)`}>
          <Glyph name="lock" />
          Lock
        </ToolbarButton>

        {macOS ? null : (
          <>
            <span className="bg-hairline mx-1 h-5 w-px shrink-0" aria-hidden="true" />
            <WindowControls />
          </>
        )}
      </div>
    </div>
  );
}
