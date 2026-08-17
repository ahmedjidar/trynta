/**
 * Command palette — components.md §14, SPEC-V1 §7.1.
 *
 * ## What it searches
 *
 * Items and actions in one list, which is what the design draws and what the title bar's
 * placeholder promises ("Search vault, actions, tags"). Item rows come from the same
 * `items_list` command the vault list uses, so the ranking is the Rust index's rather than
 * a second, differently-behaved matcher in TypeScript. Action rows are matched on their
 * own label locally, because they are not in the index and never should be.
 *
 * ## What it does not do
 *
 * No row copies a password. §14 gives every result the same shape, and a palette row that
 * copies on Enter would put a secret one keystroke away from a fuzzy match — the wrong
 * side of §4.3's deliberate friction. Choosing an item opens it; the detail pane's Copy
 * action stays the way to copy.
 *
 * ## Keyboard
 *
 * §14 pre-highlights the first result, which implies Enter runs it. Up/Down move,
 * Escape and a click on the veil dismiss. The list is a `listbox` with
 * `aria-activedescendant` rather than moving DOM focus, so the query input keeps focus
 * and typing continues to filter — the behaviour the design's autofocused input implies.
 */

import { useEffect, useId, useMemo, useRef, useState } from 'react';

import { IdentityTile } from '../../components/IdentityTile';
import { useItems } from '../items/useItems';
import { useNavigation } from '../../app/navigation';
import type { Surface } from '../../app/navigation';

/** An action row: a label, the surface it opens, and its match text. */
interface Action {
  id: string;
  label: string;
  surface: Surface;
}

const ACTIONS: readonly Action[] = [
  { id: 'action:generator', label: 'Open the generator', surface: 'generator' },
  { id: 'action:security', label: 'Open the security report', surface: 'security' },
  { id: 'action:settings', label: 'Open settings', surface: 'settings' },
  { id: 'action:vault', label: 'Show all items', surface: 'vault' },
];

export interface PaletteProps {
  /** Dismiss without running anything. */
  onClose: () => void;
  /** Lock the vault — the one action that is not a navigation. */
  onLock: () => void;
}

export function Palette({ onClose, onLock }: PaletteProps) {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const listId = useId();
  const field = useRef<HTMLInputElement>(null);

  const go = useNavigation((s) => s.go);
  const select = useNavigation((s) => s.select);

  // Every item, ranked by the Rust index. The palette's own query is applied below
  // rather than through this hook, because the hook's query key is the vault list's
  // and typing here must not disturb the list behind the veil.
  const items = useItems();

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const matches = (text: string) => needle === '' || text.toLowerCase().includes(needle);

    const itemRows = (items.data ?? [])
      .filter((item) => matches(item.title) || matches(item.subtitle ?? ''))
      .slice(0, 8)
      .map((item) => ({
        id: item.id,
        name: item.title,
        kind: 'Item' as const,
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
      kind: 'Action' as const,
      icon: null,
      run: () => {
        go(action.surface);
        onClose();
      },
    }));

    if (matches('Lock the vault')) {
      actionRows.push({
        id: 'action:lock',
        name: 'Lock the vault',
        kind: 'Action' as const,
        icon: null,
        run: () => {
          onClose();
          onLock();
        },
      });
    }

    return [...itemRows, ...actionRows];
  }, [items.data, query, go, select, onClose, onLock]);

  useEffect(() => {
    field.current?.focus();
  }, []);

  // Clamp rather than reset: a query that narrows the list must not leave the
  // highlight past its end, and resetting to 0 on every keystroke would fight a
  // user who has already moved down.
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
      className="veil"
      // The veil dismisses on click, as §14 requires. It is not a button: it is a
      // backdrop, and the dialog below it is what receives focus.
      onClick={onClose}
      role="presentation"
    >
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        onClick={(event) => {
          event.stopPropagation();
        }}
      >
        <input
          ref={field}
          className="palette__query"
          type="text"
          value={query}
          placeholder="Search items and actions…"
          aria-label="Search items and actions"
          aria-controls={listId}
          aria-activedescendant={rows[index] ? `${listId}-${rows[index].id}` : undefined}
          onKeyDown={onKeyDown}
          onChange={(event) => {
            setQuery(event.target.value);
          }}
        />

        <div className="palette__results" id={listId} role="listbox" aria-label="Results">
          {rows.length === 0 ? (
            // §14 records that the prototype has no empty state because "actions always
            // match", and specifies the row to add. It can be reached here: a query that
            // matches no item and no action label.
            <p className="palette__empty">No matches</p>
          ) : (
            rows.map((row, position) => (
              <div
                key={row.id}
                id={`${listId}-${row.id}`}
                className="palette__row"
                role="option"
                aria-selected={position === index}
                data-active={position === index ? '' : undefined}
                onMouseDown={(event) => {
                  // Mouse *down*, so the input does not lose focus and re-render
                  // before the click lands.
                  event.preventDefault();
                  row.run();
                }}
              >
                {row.icon === null ? (
                  <span className="palette__tile" aria-hidden="true" />
                ) : (
                  <IdentityTile icon={row.icon} size="sm" title={row.name} />
                )}
                <span className="palette__name">{row.name}</span>
                <span className="palette__kind">{row.kind}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
