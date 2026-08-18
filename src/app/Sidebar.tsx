/**
 * Source list — HO-002 `components/Sidebar.tsx`.
 *
 * ## Two departures
 *
 * **A listbox, not a stack of buttons.** HO-002 renders each row as a `<button>`, so a
 * twelve-row sidebar costs twelve tab presses to walk past. This is one tab stop with
 * arrow keys inside it, which is what a listbox is for. Selection follows focus, correct
 * for a navigation list: the design has no "focused but not selected" state, so adding one
 * would be inventing an interaction.
 *
 * **No People row.** HO-002's Tools group has People with an invite badge. People, invites,
 * roles and share links are SPEC-V2/V3, so the row is absent rather than disabled — a
 * sidebar entry that opens nothing is worse than one that is not there yet.
 *
 * The footer reads "This device only" rather than HO-002's "Encrypted · synced 2 min ago":
 * there is no sync in V1 (SPEC-V1 §1), so the design's copy would be a claim about a
 * feature that does not exist.
 */

import { useRef } from 'react';
import type { KeyboardEvent } from 'react';

import { CATEGORIES, sameSource, useNavigation } from './navigation';
import { cn } from '../lib/cn';
import { Glyph } from '../components/Glyph';
import type { GlyphName } from '../components/Glyph';
import type { ItemSourceDto, VaultSummaryDto } from '../ipc';

export interface SidebarProps {
  /** Vaults to list, from `vaults_list`. */
  vaults: readonly VaultSummaryDto[];
  /** Per-source counts for the trailing numerals. */
  counts: Readonly<Record<string, number>>;
  /** Items the last security report flagged, for the risk badge. */
  riskCount: number;
}

interface Row {
  key: string;
  label: string;
  // `| undefined` on every optional: `exactOptionalPropertyTypes` is on, so
  // `count?: number` means "absent or a number", not "absent, a number, or
  // undefined" — and a count that has not loaded yet is genuinely undefined.
  source?: ItemSourceDto | undefined;
  surface?: 'generator' | 'security' | 'settings' | undefined;
  count?: number | undefined;
  badge?: number | undefined;
  colorToken?: string | undefined;
  glyph?: GlyphName | undefined;
}

export function Sidebar({ vaults, counts, riskCount }: SidebarProps) {
  const source = useNavigation((s) => s.source);
  const surface = useNavigation((s) => s.surface);
  const setSource = useNavigation((s) => s.setSource);
  const go = useNavigation((s) => s.go);
  const listRef = useRef<HTMLDivElement>(null);

  const groups: { label: string; rows: Row[] }[] = [
    {
      label: 'Vaults',
      rows: [
        {
          key: 'all',
          label: 'All items',
          source: { source: 'all' },
          count: counts.all,
          glyph: 'all' as const,
        },
        ...vaults.map((v) => ({
          key: `vault:${v.id}`,
          label: v.name,
          source: { source: 'vault' as const, id: v.id },
          count: v.itemCount,
          colorToken: v.colorToken,
        })),
      ],
    },
    {
      label: 'Library',
      rows: [
        {
          key: 'favorites',
          label: 'Favourites',
          source: { source: 'favorites' },
          count: counts.favorites,
          glyph: 'favorite' as const,
        },
        ...CATEGORIES.map((c) => ({
          key: `category:${c.kind}`,
          label: c.label,
          source: { source: 'category' as const, kind: c.kind },
          count: counts[c.kind],
          glyph: c.glyph,
        })),
      ],
    },
    {
      label: 'Tools',
      rows: [
        {
          key: 'generator',
          label: 'Generator',
          surface: 'generator' as const,
          glyph: 'generate' as const,
        },
        {
          key: 'security',
          label: 'Security report',
          surface: 'security' as const,
          badge: riskCount,
          glyph: 'security' as const,
        },
        {
          key: 'settings',
          label: 'Settings',
          surface: 'settings' as const,
          glyph: 'settings' as const,
        },
      ],
    },
  ];

  const flat = groups.flatMap((g) => g.rows);
  const activeKey =
    surface === 'vault'
      ? (flat.find((r) => r.source && sameSource(r.source, source))?.key ?? 'all')
      : surface;

  const activate = (row: Row) => {
    if (row.source) {
      setSource(row.source);
      go('vault');
    } else if (row.surface) {
      go(row.surface);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const at = flat.findIndex((r) => r.key === activeKey);
    const last = flat.length - 1;
    const next =
      event.key === 'ArrowDown'
        ? Math.min(at + 1, last)
        : event.key === 'ArrowUp'
          ? Math.max(at - 1, 0)
          : event.key === 'Home'
            ? 0
            : event.key === 'End'
              ? last
              : null;
    if (next === null) return;

    event.preventDefault();
    const row = flat[next];
    if (row) {
      activate(row);
      // Move real focus with the selection so the ring follows it and a screen
      // reader announces the new row.
      listRef.current?.querySelector<HTMLElement>(`[data-key="${row.key}"]`)?.focus();
    }
  };

  return (
    <nav
      className="border-hairline bg-surface-sidebar vibrancy flex w-60 shrink-0 flex-col border-r"
      aria-label="Sources"
    >
      <div
        className="min-h-0 flex-1 overflow-y-auto px-3 pt-3"
        ref={listRef}
        role="listbox"
        aria-label="Sources"
        onKeyDown={onKeyDown}
      >
        {groups.map((group, groupIndex) => (
          <div key={group.label} className="flex flex-col gap-[var(--row-gap)]">
            <h2
              className={cn(
                'text-micro tracking-label text-text-caption-aa flex h-6 items-center px-2 font-bold uppercase',
                groupIndex > 0 && 'mt-4',
              )}
            >
              {group.label}
            </h2>
            {group.rows.map((row) => {
              const selected = row.key === activeKey;
              return (
                <div
                  key={row.key}
                  data-key={row.key}
                  role="option"
                  aria-selected={selected}
                  // One tab stop for the list: only the selected row is tabbable.
                  tabIndex={selected ? 0 : -1}
                  className={cn(
                    'text-body duration-quick flex h-[30px] w-full items-center gap-2.5 rounded-sm px-2 transition-colors',
                    selected
                      ? 'bg-surface-selected text-text-primary font-semibold'
                      : 'text-text-secondary hover:bg-surface-hover cursor-pointer font-medium',
                  )}
                  onClick={() => {
                    activate(row);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      activate(row);
                    }
                  }}
                >
                  <span
                    className={cn(
                      'flex h-4 w-4 shrink-0 items-center justify-center',
                      selected ? 'text-accent' : 'text-text-caption-aa',
                    )}
                    aria-hidden="true"
                  >
                    {row.colorToken ? (
                      <span className="swatch h-2 w-2 rounded-xs" data-accent={row.colorToken} />
                    ) : row.glyph ? (
                      <Glyph name={row.glyph} />
                    ) : null}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-left">{row.label}</span>
                  {row.badge ? (
                    <span className="bg-status-warning-subtle text-micro text-status-warning-text h-[18px] min-w-[18px] rounded-full px-1.5 text-center leading-[18px] font-bold tabular-nums">
                      {row.badge}
                    </span>
                  ) : row.count === undefined ? null : (
                    <span className="text-micro text-text-caption-aa font-medium tabular-nums">
                      {row.count}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <footer className="border-hairline text-micro text-text-caption-aa flex h-10 shrink-0 items-center gap-2 border-t px-5">
        <span className="bg-accent h-1.5 w-1.5 shrink-0 rounded-full" aria-hidden="true" />
        {/* SPEC-V1 §1: no sync in V1. The design's footer slot says something true
            rather than implying a feature that does not exist. */}
        <span>This device only</span>
      </footer>
    </nav>
  );
}
