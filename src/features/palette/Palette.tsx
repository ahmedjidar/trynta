/**
 * Command palette — SPEC-V1 §7.9.
 *
 * ## What it searches
 *
 * Items and actions in one list. Item rows come from the same `items_list` command the
 * vault list uses, so the ranking is the Rust index's rather than a second, differently
 * behaved matcher in TypeScript. Action rows match on their own label locally, because they
 * are not in the index and never should be.
 *
 * ## What it does not do
 *
 * No row copies a password. The design gives every result the same shape and its actions are
 * navigations; a palette row that copied on Enter would put a secret one fuzzy match away
 * from the wrong side of §4.3's deliberate friction. Choosing an item opens it.
 *
 * ## Keyboard
 *
 * The design pre-highlights the first result, which implies Enter runs it. Up/Down move,
 * Escape and a click on the veil dismiss. The list is a `listbox` with
 * `aria-activedescendant` rather than moving DOM focus, so the query keeps focus and typing
 * continues to filter — the behaviour the design's autofocused input implies but does not wire.
 */

import { useEffect, useMemo, useRef, useState } from 'react';

import { IdentityTile } from '../../components/IdentityTile';
import { Glyph } from '../../components/Glyph';
import { useItems, useVaults } from '../items/useItems';
import { useNavigation } from '../../app/navigation';
import type { Surface } from '../../app/navigation';
import { cn } from '../../lib/cn';

interface Action {
  id: string;
  label: string;
  surface: Surface;
}

const ACTIONS: readonly Action[] = [
  { id: 'action:generator', label: 'Generate a new password', surface: 'generator' },
  { id: 'action:security', label: 'Run security report', surface: 'security' },
  { id: 'action:settings', label: 'Open settings', surface: 'settings' },
  { id: 'action:vault', label: 'Show all items', surface: 'vault' },
];

export interface PaletteProps {
  /** Dismiss without running anything. */
  onClose: () => void;
  /** Lock the vault — the one action that is not a navigation. */
  onLock: () => void;
  /** The platform's modifier label, for the Lock row's hint. Never hardcoded (§8). */
  modifierKey: string;
}

export function Palette({ onClose, onLock, modifierKey }: PaletteProps) {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const field = useRef<HTMLInputElement>(null);

  const go = useNavigation((s) => s.go);
  const select = useNavigation((s) => s.select);

  // Every item, ranked by the Rust index. The palette's own query filters below rather
  // than through this hook, so typing here does not disturb the list behind the veil.
  const items = useItems();
  const vaults = useVaults();

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const matches = (text: string) => needle === '' || text.toLowerCase().includes(needle);

    const itemRows = (items.data ?? [])
      .filter((item) => matches(item.title) || matches(item.subtitle ?? ''))
      .slice(0, 6)
      .map((item) => ({
        id: item.id,
        name: item.title,
        // The trailing label is the owning vault's name, not the subtitle.
        kind: vaults.data?.find((v) => v.id === item.vaultId)?.name ?? 'Item',
        icon: item.icon,
        run: () => {
          select(item.id);
          go('vault');
          onClose();
        },
      }));

    const actionRows = ACTIONS.filter((action) => matches(action.label)).map((action) => ({
      id: action.id,
      name: action.label,
      kind: 'Action',
      icon: null,
      run: () => {
        go(action.surface);
        onClose();
      },
    }));

    if (matches('Lock vault')) {
      actionRows.push({
        id: 'action:lock',
        name: 'Lock vault',
        kind: `${modifierKey}L`,
        icon: null,
        run: () => {
          onClose();
          onLock();
        },
      });
    }

    return [...itemRows, ...actionRows];
  }, [items.data, vaults.data, query, go, select, onClose, onLock, modifierKey]);

  useEffect(() => {
    field.current?.focus();
  }, []);

  // Clamp rather than reset: a narrowing query must not leave the highlight past the end,
  // and resetting to 0 on every keystroke would fight a user who has already moved down.
  const index = Math.min(active, Math.max(rows.length - 1, 0));

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setActive(Math.min(index + 1, rows.length - 1));
      return;
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setActive(Math.max(index - 1, 0));
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      rows[index]?.run();
    }
  }

  return (
    <div
      role="presentation"
      onClick={onClose}
      className="animate-veil-in bg-surface-veil veil-blur absolute inset-0 z-[6] flex justify-center pt-28"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onClick={(event) => {
          event.stopPropagation();
        }}
        className="animate-sheet-in bg-surface-panel shadow-sheet flex h-fit max-h-[420px] w-[600px] flex-col overflow-hidden rounded-xl"
      >
        <input
          ref={field}
          className="border-hairline text-input-lg text-text-primary h-[52px] shrink-0 border-b bg-transparent px-5 outline-none"
          type="text"
          value={query}
          placeholder="Search items and actions…"
          aria-label="Search items and actions"
          aria-activedescendant={rows[index] ? `palette-${rows[index].id}` : undefined}
          autoComplete="off"
          spellCheck={false}
          onKeyDown={onKeyDown}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
        />

        <div
          className="flex flex-col gap-[var(--row-gap)] overflow-y-auto p-2"
          role="listbox"
          aria-label="Results"
        >
          {rows.length === 0 ? (
            <p className="text-caption text-text-muted px-3 py-10 text-center">No matches.</p>
          ) : (
            rows.map((row, position) => (
              <div
                key={row.id}
                id={`palette-${row.id}`}
                role="option"
                aria-selected={position === index}
                onMouseDown={(event) => {
                  // Mouse *down*, so the input does not lose focus and re-render before
                  // the click lands.
                  event.preventDefault();
                  row.run();
                }}
                className={cn(
                  'duration-fast flex h-10 shrink-0 cursor-pointer items-center gap-3 rounded-md px-3 text-left transition-colors',
                  position === index ? 'bg-surface-selected' : 'hover:bg-surface-hover',
                )}
              >
                {row.icon === null ? (
                  <span className="bg-accent-subtle text-accent flex h-6 w-6 shrink-0 items-center justify-center rounded-sm">
                    <Glyph name="generate" size={12} />
                  </span>
                ) : (
                  <IdentityTile icon={row.icon} size={24} title={row.name} />
                )}
                <span className="text-body min-w-0 flex-1 truncate font-medium">{row.name}</span>
                <span className="text-micro text-text-muted shrink-0">{row.kind}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
