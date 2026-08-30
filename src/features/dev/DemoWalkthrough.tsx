// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * An autopilot for recording the product, and nothing else.
 *
 * One click drives the app through the tour a presenter would give — the list,
 * an item with a live code, the security report, the generator, then the three
 * imported themes — at a pace a camera can follow. It exists so a recording does
 * not depend on somebody clicking accurately while also talking.
 *
 * ## This is not the guided tour
 *
 * `features/tour/` teaches a first-run user, anchors cards to real elements and
 * waits for them. This waits for nobody, explains nothing, and shows no UI of its
 * own once it starts. They share no code and neither knows about the other.
 *
 * ## Why it cannot reach a release build
 *
 * The button renders only when {@link useTour}'s `replay` is true, and that flag
 * is `services::tour::DEV_REPLAY` — `cfg!(debug_assertions)`, decided by the
 * profile the binary was compiled with. Not an environment variable, not
 * `import.meta.env.DEV`: the frontend is built by `pnpm build` in *both* profiles,
 * so Vite's flag is false even in the debug binary and would gate exactly the
 * wrong way round. The Rust flag tracks the artefact, which is the thing that has
 * to be true.
 *
 * ## What it deliberately does not do
 *
 * No reveal and no copy. Both are real product paths, but one puts a plaintext on
 * screen and the other on the system clipboard, and an autopilot should not do
 * either on its own — least of all while a screen recorder is running. Everything
 * here is navigation, scrolling and theme switching.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { useNavigation } from '../../app/navigation';
import { useThemeStore } from '../../theme/store';
import { useTour } from '../tour/store';
import type { ItemSummaryDto } from '../../ipc';

/** Beats, in milliseconds. Slow enough to read, short enough to keep a hook moving. */
const BEAT = {
  settle: 900,
  read: 1900,
  linger: 2600,
  theme: 2200,
} as const;

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Scroll a pane from wherever it is to `to`, in one smooth movement.
 *
 * `scrollTo` rather than a per-frame loop: the app's panes already carry
 * `scroll-behavior: smooth`, so the browser owns the easing and it matches every
 * other scroll in the product.
 */
function glide(pane: Element | null, to: number): void {
  if (!(pane instanceof HTMLElement)) return;
  pane.scrollTo({ top: to, behavior: 'smooth' });
}

/** The scroll container of whichever surface is on screen. */
const pane = (label?: string) =>
  document.querySelector(
    label ? `[aria-label="${label}"][data-scroll-pane]` : '[data-scroll-pane]',
  );

export interface DemoWalkthroughProps {
  /** Every visible row, so the run can open something worth looking at. */
  items: readonly ItemSummaryDto[];
}

export function DemoWalkthrough({ items }: DemoWalkthroughProps) {
  const isDevBuild = useTour((s) => s.replay);
  const go = useNavigation((s) => s.go);
  const select = useNavigation((s) => s.select);
  const setTheme = useThemeStore((s) => s.setTheme);
  const setMode = useThemeStore((s) => s.setMode);
  const imported = useThemeStore((s) => s.imported);

  const [running, setRunning] = useState(false);
  const cancelled = useRef(false);
  // The run outlives the render that started it, so it reads these when it gets
  // there rather than closing over whatever they were at the first click. Written
  // in an effect, not during render — a ref assignment mid-render is a tear.
  const latest = useRef({ items, imported });
  useEffect(() => {
    latest.current = { items, imported };
  }, [items, imported]);

  useEffect(
    () => () => {
      cancelled.current = true;
    },
    [],
  );

  const play = useCallback(async () => {
    cancelled.current = false;
    setRunning(true);

    /** Every await goes through here, so Escape stops the run between any two beats. */
    const beat = async (ms: number) => {
      await sleep(ms);
      return !cancelled.current;
    };

    try {
      // ── the vault, top to bottom ──
      go('vault');
      select(null);
      if (!(await beat(BEAT.settle))) return;

      const list = document.querySelector('[aria-label="Items"] .overflow-y-auto');
      glide(list, 520);
      if (!(await beat(BEAT.read))) return;
      glide(list, 1180);
      if (!(await beat(BEAT.read))) return;
      glide(list, 0);
      if (!(await beat(BEAT.settle))) return;

      // ── one item, chosen because it has a code ticking on it ──
      const withTotp = latest.current.items.find((i) => i.hasTotp) ?? latest.current.items[0];
      if (withTotp) {
        select(withTotp.id);
        if (!(await beat(BEAT.linger))) return;
        glide(pane('Item detail'), 420);
        if (!(await beat(BEAT.read))) return;
        glide(pane('Item detail'), 0);
        if (!(await beat(BEAT.settle))) return;
      }

      // ── what the vault is worth ──
      go('security');
      if (!(await beat(BEAT.linger))) return;
      glide(pane('Security report'), 460);
      if (!(await beat(BEAT.read))) return;
      glide(pane('Security report'), 980);
      if (!(await beat(BEAT.read))) return;
      glide(pane('Security report'), 0);
      if (!(await beat(BEAT.settle))) return;

      // ── where new ones come from ──
      go('generator');
      if (!(await beat(BEAT.linger))) return;
      glide(pane('Generator'), 380);
      if (!(await beat(BEAT.read))) return;
      glide(pane('Generator'), 0);
      if (!(await beat(BEAT.settle))) return;

      // ── the same screen, however many ways are installed ──
      go('settings');
      if (!(await beat(BEAT.settle))) return;
      for (const theme of latest.current.imported) {
        // The mode first, and it is not decoration: `theme/loader.ts` scopes a
        // theme's rule to `:root[data-theme="light"]` or `:root:not([data-theme])`
        // by its own `mode`, so activating a dark theme while the app is in light
        // mode installs a rule that matches nothing. Two of the three themes
        // changed nothing on screen until this line existed.
        await setMode(theme.mode);
        await setTheme(theme.id);
        if (!(await beat(BEAT.theme))) return;
      }
      await setTheme(null);
      await setMode('dark');
      if (!(await beat(BEAT.theme))) return;
      await setMode('light');
      if (!(await beat(BEAT.settle))) return;

      // ── back where it started, list and all ──
      select(null);
      go('vault');
      await beat(BEAT.settle);
    } finally {
      setRunning(false);
    }
  }, [go, select, setTheme, setMode]);

  // Escape stops it. A run that cannot be interrupted is not usable on camera.
  useEffect(() => {
    if (!running) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      cancelled.current = true;
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
  }, [running]);

  // Not a debug build, or already playing: nothing on screen. The control removes
  // itself for the duration precisely because the point is to record what is behind it.
  if (!isDevBuild || running) return null;

  return (
    <button
      type="button"
      data-focus-ring
      onClick={() => {
        void play();
      }}
      title="Dev build only — drives the app through a recording pass. Escape stops it."
      // `bottom-14` clears the sidebar's own `h-10` footer, which `bottom-4` sat on.
      className="border-hairline bg-surface-raised text-text-secondary shadow-card duration-hover hover:text-text-primary fixed bottom-14 left-4 z-[60] flex h-8 items-center gap-2 rounded-full border px-3 transition-colors"
    >
      <span className="bg-accent h-1.5 w-1.5 rounded-full" aria-hidden="true" />
      <span className="text-control font-medium">Demo run</span>
    </button>
  );
}
