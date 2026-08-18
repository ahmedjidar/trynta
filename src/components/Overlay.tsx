/**
 * Shared overlay chrome: blurred scrim, click-outside to dismiss, spring entry.
 *
 * This is a scrim and a card. It is **not** a window, and it must never grow traffic
 * lights, a desk background or a second border radius around the app — nesting a window
 * inside the window is the one mistake this design invites and it is always wrong.
 *
 * ## Escape, and where focus goes
 *
 * Each overlay owns its own Escape handler rather than sharing a global one: they mount
 * independently, and a single global handler closes whichever one it believes is open.
 * The scrim is `role="presentation"` with the dialog inside it carrying `aria-modal`,
 * which is what keeps the click-outside target invisible to assistive technology.
 */

import { useEffect, useRef } from 'react';
import type { KeyboardEvent, ReactNode } from 'react';

import { cn } from '../lib/cn';

export interface OverlayProps {
  /** Called on Escape or a click on the scrim. */
  onDismiss: () => void;
  /** Accessible name for the dialog. */
  label: string;
  /** `palette` sits 112px down, `sheet` 64px, `centre` is vertically centred. */
  placement?: 'palette' | 'sheet' | 'centre';
  children: ReactNode;
}

export function Overlay({ onDismiss, label, placement = 'sheet', children }: OverlayProps) {
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Focus the dialog itself if nothing inside it took focus, so Escape reaches the
    // handler below and a screen reader announces the dialog rather than the pane behind.
    const node = dialog.current;
    if (node && !node.contains(document.activeElement)) node.focus();
  }, []);

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onDismiss();
    }
  }

  return (
    <div
      role="presentation"
      onClick={onDismiss}
      onKeyDown={onKeyDown}
      className={cn(
        'animate-veil-in bg-surface-veil veil-blur absolute inset-0 z-[6] flex justify-center',
        placement === 'centre' ? 'items-center' : 'items-start',
        placement === 'palette' && 'pt-28',
        placement === 'sheet' && 'pt-16',
      )}
    >
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        tabIndex={-1}
        onClick={(event) => {
          event.stopPropagation();
        }}
        className="animate-sheet-in bg-surface-panel shadow-sheet flex flex-col overflow-hidden rounded-xl outline-none"
      >
        {children}
      </div>
    </div>
  );
}

/** Sheet header: icon chip, title, one-line rationale. */
export function SheetHeader({
  icon,
  title,
  sub,
}: {
  /** Glyph for the accent chip. */
  icon: ReactNode;
  /** Sheet title. */
  title: ReactNode;
  /** One line saying what the sheet will do. */
  sub: string;
}) {
  return (
    <header className="border-hairline flex h-14 shrink-0 items-center gap-3 border-b px-5">
      <span className="bg-accent-subtle text-accent flex h-8 w-8 shrink-0 items-center justify-center rounded-md">
        {icon}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-heading tracking-title truncate font-bold">{title}</div>
        <div className="text-chip text-text-muted truncate leading-[15px]">{sub}</div>
      </div>
    </header>
  );
}

/** Sheet footer: status hint on the left, Cancel plus the confirm action on the right. */
export function SheetFooter({ hint, children }: { hint: ReactNode; children: ReactNode }) {
  return (
    <footer className="border-hairline bg-surface-raised flex h-[60px] shrink-0 items-center gap-2.5 border-t px-5">
      <span className="text-chip text-text-muted flex items-center gap-1.5">{hint}</span>
      <div className="flex-1" />
      {children}
    </footer>
  );
}
