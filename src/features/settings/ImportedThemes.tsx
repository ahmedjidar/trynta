// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The themes the user imported, as a list they can actually pick from.
 *
 * Importing worked and storage worked, and the screen said "2 imported. Pick one from
 * the Theme row above" while the Theme row held nothing but Dark, Light and System.
 * From the outside, importing a theme did nothing at all.
 *
 * There were two reasons and this file is only one of them. There was no list — that
 * is what this adds. And the store the list reads had no themes in it either: the
 * catalogue is fetched once at mount, `theme_list` returns no imported themes while
 * the vault is locked because their values are in the encrypted settings blob, and
 * nothing re-read it after unlock. Settings still showed the right count, because
 * that comes from `settings_get`. See the `unlocked` effect in `app/App.tsx`; without
 * it this component renders `null` forever and looks like it was never mounted.
 *
 * A separate list rather than more segments in the Theme row, for two reasons. The
 * row is a three-way control over *mode* — dark, light, follow the system — and an
 * imported theme is not a fourth mode; it replaces the palette **within** a mode. And
 * a segmented control has to fit its segments side by side, so it stops working at
 * about four and someone with a dozen themes would get an unreadable row. The
 * built-ins keep the row to themselves and nothing here can shadow or remove them.
 */

import { useEffect, useState } from 'react';

import { CopyAction } from '../../components/Bits';
import { Glyph } from '../../components/Glyph';
import { GroupedList, GroupedRow } from '../../components/GroupedList';
import { themeDelete } from '../../ipc';
import type { ThemeDto } from '../../ipc';
import { applySwatches } from '../../theme/loader';
import { useThemeStore } from '../../theme/store';
import { cn } from '../../lib/cn';

interface ThemeRowProps {
  theme: ThemeDto;
  /** Position in the list, which is how the swatch finds its colours. */
  index: number;
  /** Whether this is the theme currently applied. */
  active: boolean;
  /** A write is in flight; nothing in the list should be clickable. */
  busy: boolean;
  onPick: () => void;
  onRemove: () => void;
}

/**
 * One row: the theme's swatch, its name and id, and the two things you can do to it.
 *
 * The swatch is painted in the theme's **own** colours, because a preview drawn in
 * the current palette says nothing about what you are picking. Those colours are
 * data, not design, so they cannot be a class in the token layer — and the React
 * `style` prop is banned in this repo for a reason worth restating: the production
 * CSP is `style-src 'self'`, so an inline style works under `pnpm dev` and silently
 * disappears from the packaged build. `applySwatches` publishes them as custom
 * properties through the same constructible stylesheet the theme loader uses, and
 * `data-swatch` is how this row finds its pair.
 */
function ThemeRow({ theme, index, active, busy, onPick, onRemove }: ThemeRowProps) {
  const [confirming, setConfirming] = useState(false);

  return (
    <GroupedRow className="min-h-[56px] gap-3 py-2" data-swatch={index}>
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-3 text-left"
        onClick={onPick}
        disabled={busy}
        aria-pressed={active}
      >
        <span
          className="border-hairline grid h-8 w-8 shrink-0 place-items-center rounded-md border bg-[var(--swatch-bg)]"
          aria-hidden="true"
        >
          <span className="h-3 w-3 rounded-full bg-[var(--swatch-accent)]" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="text-body block truncate font-semibold">{theme.name}</span>
          <span className="text-chip text-text-muted mt-0.5 block truncate">
            {theme.id} · {theme.mode === 'dark' ? 'Dark' : 'Light'} ·{' '}
            {Object.keys(theme.tokens).length} tokens
          </span>
        </span>
        {/* The check reserves its box whether or not it is drawn, so activating a
          theme does not shift the row it sits in. */}
        <span className="text-accent grid h-5 w-5 shrink-0 place-items-center">
          {active ? <Glyph name="check" size={16} /> : null}
        </span>
      </button>

      {confirming ? (
        // The same two-step and the same tone as deleting a vault, rather than a
        // second dialect for the same kind of action.
        <span className="flex shrink-0 items-center gap-1.5">
          <CopyAction
            className="h-[26px] rounded-md px-[10px]"
            disabled={busy}
            onClick={() => {
              setConfirming(false);
            }}
          >
            Keep
          </CopyAction>
          <CopyAction
            className="h-[26px] rounded-md px-[10px]"
            data-tone="danger"
            disabled={busy}
            onClick={onRemove}
          >
            Remove
          </CopyAction>
        </span>
      ) : (
        <button
          type="button"
          className={cn(
            'text-text-muted hover:text-text-primary duration-hover grid h-7 w-7 shrink-0',
            'place-items-center rounded-md transition-colors',
          )}
          onClick={() => {
            setConfirming(true);
          }}
          disabled={busy}
          aria-label={`Remove ${theme.name}`}
        >
          <Glyph name="close" size={14} />
        </button>
      )}
    </GroupedRow>
  );
}

export interface ImportedThemesProps {
  /** Report a failure to the toast. */
  onFailed: (message: string) => void;
  /** Report a success to the toast. */
  onDone: (what: string) => void;
}

/**
 * The imported-theme list, with activation and removal.
 *
 * Renders nothing when there is nothing imported — an empty list under a row that
 * already explains what importing is would be two ways of saying the same nothing.
 *
 * @param props - See {@link ImportedThemesProps}.
 */
export function ImportedThemes({ onFailed, onDone }: ImportedThemesProps) {
  const imported = useThemeStore((s) => s.imported);
  const activeId = useThemeStore((s) => s.activeId);
  const setTheme = useThemeStore((s) => s.setTheme);
  const refresh = useThemeStore((s) => s.refresh);
  const [busy, setBusy] = useState(false);

  // Republished whenever the set *or* the selection changes: a removal must not leave
  // the row below it wearing the removed theme's colours, and returning to Built-in is
  // the moment `applySwatches` can re-snapshot the built-in palette.
  useEffect(() => {
    applySwatches(imported);
  }, [imported, activeId]);

  if (imported.length === 0) return null;

  return (
    <div className="mt-4">
      <div className="text-chip text-text-muted mb-2 px-1">
        Your themes — these replace the palette inside the mode each was made for.
      </div>
      <GroupedList>
        {/* Getting back to the built-in palette has to be one click, or activating a
          theme is a one-way door. */}
        <GroupedRow className="min-h-[56px] gap-3 py-2" data-swatch="builtin">
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center gap-3 text-left"
            disabled={busy}
            aria-pressed={activeId === null}
            onClick={() => {
              setBusy(true);
              void setTheme(null).finally(() => {
                setBusy(false);
              });
            }}
          >
            {/* The snapshot, not the live tokens: an active imported theme redefines
              `--accent` on the root, so `bg-accent` here would paint the Built-in
              swatch in the colours of the theme you are trying to leave. */}
            <span
              className="border-hairline grid h-8 w-8 shrink-0 place-items-center rounded-md border bg-[var(--swatch-bg)]"
              aria-hidden="true"
            >
              <span className="h-3 w-3 rounded-full bg-[var(--swatch-accent)]" />
            </span>
            <span className="min-w-0 flex-1">
              <span className="text-body block truncate font-semibold">Built-in</span>
              <span className="text-chip text-text-muted mt-0.5 block truncate">
                Trynta&rsquo;s own palette
              </span>
            </span>
            <span className="text-accent grid h-5 w-5 shrink-0 place-items-center">
              {activeId === null ? <Glyph name="check" size={16} /> : null}
            </span>
          </button>
        </GroupedRow>

        {imported.map((theme, index) => (
          <ThemeRow
            key={theme.id}
            theme={theme}
            index={index}
            active={theme.id === activeId}
            busy={busy}
            onPick={() => {
              setBusy(true);
              void setTheme(theme.id).finally(() => {
                setBusy(false);
              });
            }}
            onRemove={() => {
              setBusy(true);
              themeDelete(theme.id).then(
                () => {
                  // `theme_delete` clears the stored selection when it removes the
                  // active theme, so re-reading the catalogue is what puts the applied
                  // CSS back in agreement with what is stored.
                  void refresh().finally(() => {
                    setBusy(false);
                  });
                  onDone(`Removed ${theme.name}`);
                },
                () => {
                  setBusy(false);
                  onFailed(`${theme.name} could not be removed.`);
                },
              );
            }}
          />
        ))}
      </GroupedList>
    </div>
  );
}
