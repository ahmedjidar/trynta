/**
 * Source list — components.md §3.
 *
 * The handoff flags this row set as a gap: they are divs with no roving tabindex, and
 * should be options in a listbox with the focus ring. Built that way — one tab stop
 * for the whole list, arrow keys inside it. That is what a listbox is for, and it is
 * what stops a twelve-row sidebar costing twelve tab presses to walk past.
 *
 * Selection follows focus, which is correct for a navigation list: the design has no
 * "focused but not selected" state, so introducing one would be inventing an
 * interaction rather than implementing one.
 */

import { useRef } from 'react';
import type { KeyboardEvent } from 'react';

import { CATEGORIES, sameSource, useNavigation } from './navigation';
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
        { key: 'all', label: 'All items', source: { source: 'all' }, count: counts.all },
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
        },
        ...CATEGORIES.map((c) => ({
          key: `category:${c.kind}`,
          label: c.label,
          source: { source: 'category' as const, kind: c.kind },
          count: counts[c.kind],
        })),
      ],
    },
    {
      label: 'Tools',
      rows: [
        { key: 'generator', label: 'Generator', surface: 'generator' as const },
        { key: 'security', label: 'Security', surface: 'security' as const, badge: riskCount },
        { key: 'settings', label: 'Settings', surface: 'settings' as const },
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
    <nav className="sidebar" aria-label="Sources">
      <div
        className="sidebar__scroll"
        ref={listRef}
        role="listbox"
        aria-label="Sources"
        onKeyDown={onKeyDown}
      >
        {groups.map((group) => (
          <div className="sidebar__group" key={group.label}>
            <h2 className="section-label">{group.label}</h2>
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
                  className="nav-row"
                  data-selected={selected || undefined}
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
                  <span className="nav-row__slot" aria-hidden="true">
                    {row.colorToken ? (
                      <span className="vault-dot" data-color={row.colorToken} />
                    ) : null}
                  </span>
                  <span className="nav-row__label">{row.label}</span>
                  {row.badge ? (
                    <span className="nav-row__badge">{row.badge}</span>
                  ) : row.count === undefined ? null : (
                    <span className="nav-row__count">{row.count}</span>
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <footer className="sync-footer">
        <span className="sync-footer__dot" aria-hidden="true" />
        {/* SPEC-V1 §1: no sync in V1. The design's footer slot says something true
            rather than implying a feature that does not exist. */}
        <span>This device only</span>
      </footer>
    </nav>
  );
}
