/**
 * Item list column — components.md §4, SPEC-V1 §7.1.
 *
 * Closes two more of the handoff's accessibility gaps: the column is a real listbox
 * with options rather than divs, and arrow keys are bound to it rather than to
 * `window`. The handoff's note — *"arrow keys are bound at window level"* — is a bug
 * with a specific symptom: pressing Down while typing in the search field moves the
 * list selection instead of the caret.
 *
 * SPEC-V1 §7.1 requires ⌘C / Ctrl+C to copy the selected item's primary secret
 * **without opening it**. That happens entirely in Rust (`item_copy_field`), so the
 * plaintext never enters the webview — CLAUDE.md §4.3.
 *
 * Virtualisation: §7.1 asks for smooth scrolling at 10,000 items. This renders a
 * windowed slice with a spacer above and below, which is enough because every row is
 * a fixed height from the token layer (`--row-h`) — the design made that choice for
 * alignment and it happens to make windowing trivial and exact.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { KeyboardEvent } from 'react';

import { useNavigation } from '../../app/navigation';
import { IdentityTile } from '../../components/IdentityTile';
import { Spacer } from '../../components/Spacer';
import type { ItemSummaryDto, SortOrderDto } from '../../ipc';

/** Rows rendered beyond the viewport, so a fast scroll does not show gaps. */
const OVERSCAN = 6;

/** Sort labels, in the order the control cycles them (components.md §4). */
const SORTS: readonly { value: SortOrderDto; label: string }[] = [
  { value: 'recentlyUsed', label: 'Recent' },
  { value: 'alphabetical', label: 'A–Z' },
  { value: 'recentlyUpdated', label: 'Updated' },
  { value: 'dateCreated', label: 'Created' },
];

export interface ItemListProps {
  /** Rows to show, already filtered, searched and sorted by Rust. */
  items: readonly ItemSummaryDto[];
  /** Item ids the last security report flagged, for the risk dot. */
  risks: Readonly<Record<string, 'breached' | 'weak'>>;
  /** Copy the selected item's primary secret, in Rust. */
  onCopy: (id: string) => void;
  /** Open the new-item sheet. */
  onNew: () => void;
}

export function ItemList({ items, risks, onCopy, onNew }: ItemListProps) {
  const selectedId = useNavigation((s) => s.selectedId);
  const select = useNavigation((s) => s.select);
  const filters = useNavigation((s) => s.filters);
  const toggleFilter = useNavigation((s) => s.toggleFilter);
  const sort = useNavigation((s) => s.sort);
  const setSort = useNavigation((s) => s.setSort);
  const search = useNavigation((s) => s.search);

  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);
  // Measured from a rendered row rather than parsed from `--row-h`. Custom
  // properties resolve to their specified value through `getComputedStyle` — the
  // string `var(--row-item)`, not a pixel count — and the density switch changes
  // `--row-h` at runtime, so measuring is both simpler and the only thing that stays
  // correct.
  const [rowHeight, setRowHeight] = useState(0);

  useEffect(() => {
    const element = scrollRef.current;
    if (!element) return undefined;
    const measure = () => {
      setViewport(element.clientHeight);
      const row = element.querySelector<HTMLElement>('.item-row');
      if (row && row.offsetHeight > 0) setRowHeight(row.offsetHeight);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [items.length]);

  const window_ = useMemo(() => {
    // Before the first measurement, render everything. One unwindowed frame on a
    // large vault is a brief cost; guessing a row height would misplace the spacers
    // and show a visible jump on the frame after.
    if (rowHeight <= 0 || viewport <= 0) return { start: 0, end: items.length };
    const visible = Math.ceil(viewport / rowHeight) + OVERSCAN * 2;
    const start = Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN);
    return { start, end: Math.min(items.length, start + visible) };
  }, [items.length, rowHeight, scrollTop, viewport]);

  const selectedIndex = items.findIndex((i) => i.id === selectedId);

  /** Move selection and keep it in view. */
  const move = useCallback(
    (to: number) => {
      const clamped = Math.max(0, Math.min(to, items.length - 1));
      const item = items[clamped];
      if (!item) return;
      select(item.id);

      const element = scrollRef.current;
      if (!element || rowHeight <= 0) return;
      const top = clamped * rowHeight;
      if (top < element.scrollTop) element.scrollTop = top;
      else if (top + rowHeight > element.scrollTop + element.clientHeight) {
        element.scrollTop = top + rowHeight - element.clientHeight;
      }
    },
    [items, rowHeight, select],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    // Bound to the listbox, not to `window`: the handoff's gap 2. Typing Down in the
    // search field must move the caret, not the selection.
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      move(selectedIndex < 0 ? 0 : selectedIndex + 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      move(selectedIndex < 0 ? 0 : selectedIndex - 1);
    } else if (event.key === 'Home') {
      event.preventDefault();
      move(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      move(items.length - 1);
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c') {
      // §7.1: copies the primary secret without opening the item. Rust does the
      // copy; nothing here ever holds the value.
      if (selectedId) {
        event.preventDefault();
        onCopy(selectedId);
      }
    }
  };

  const currentSort = SORTS.find((s) => s.value === sort) ?? SORTS[0];

  return (
    <section className="list-column" aria-label="Items">
      <header className="list-header">
        <h1 className="list-header__title">Items</h1>
        <span className="list-header__count">{items.length}</span>
        <button
          type="button"
          className="sort-control"
          onClick={() => {
            const at = SORTS.findIndex((s) => s.value === sort);
            const next = SORTS[(at + 1) % SORTS.length];
            if (next) setSort(next.value);
          }}
          aria-label={`Sort: ${currentSort?.label ?? ''}. Change sort order`}
        >
          {currentSort?.label}
        </button>
        <button type="button" className="new-item-button" onClick={onNew} aria-label="New item">
          +
        </button>
      </header>

      {/* Quick filters (§7.1). `shared` is V2 and is deliberately absent rather than
          present and inert — "never a toggle that does nothing" (§7.5). */}
      <div className="filter-bar" role="group" aria-label="Quick filters">
        <button
          type="button"
          className="filter-chip"
          aria-pressed={filters.weak}
          data-selected={filters.weak || undefined}
          onClick={() => {
            toggleFilter('weak');
          }}
        >
          Weak
        </button>
        <button
          type="button"
          className="filter-chip"
          aria-pressed={filters.hasTotp}
          data-selected={filters.hasTotp || undefined}
          onClick={() => {
            toggleFilter('hasTotp');
          }}
        >
          Has 2FA
        </button>
      </div>

      <div
        className="list-scroll"
        ref={scrollRef}
        role="listbox"
        aria-label="Items"
        aria-activedescendant={selectedId ? `item-${selectedId}` : undefined}
        tabIndex={0}
        onKeyDown={onKeyDown}
        onScroll={(event) => {
          setScrollTop(event.currentTarget.scrollTop);
        }}
      >
        {items.length === 0 ? (
          <p className="list-empty">
            {search === '' ? 'Nothing here yet.' : `No items match “${search}”.`}
          </p>
        ) : (
          <>
            <Spacer height={window_.start * rowHeight} />
            {items.slice(window_.start, window_.end).map((item) => (
              <ItemRow
                key={item.id}
                item={item}
                risk={risks[item.id]}
                selected={item.id === selectedId}
                onSelect={() => {
                  select(item.id);
                }}
              />
            ))}
            <Spacer height={(items.length - window_.end) * rowHeight} />
          </>
        )}
      </div>

      <footer className="kbd-footer">
        <span>
          <kbd className="kbd">↑↓</kbd> navigate
        </span>
        <span>
          <kbd className="kbd">↵</kbd> open
        </span>
      </footer>
    </section>
  );
}

interface ItemRowProps {
  item: ItemSummaryDto;
  risk: 'breached' | 'weak' | undefined;
  selected: boolean;
  onSelect: () => void;
}

function ItemRow({ item, risk, selected, onSelect }: ItemRowProps) {
  return (
    <div
      id={`item-${item.id}`}
      role="option"
      aria-selected={selected}
      className="item-row"
      data-selected={selected || undefined}
      onClick={onSelect}
    >
      <IdentityTile icon={item.icon} title={item.title} />
      <span className="item-row__labels">
        <span className="item-row__name">{item.title}</span>
        <span className="item-row__sub">{item.subtitle ?? ''}</span>
      </span>
      <span className="item-row__indicators">
        {item.hasTotp ? (
          <span className="totp-pill" aria-label="Has a one-time code">
            2FA
          </span>
        ) : null}
        {risk ? (
          <span
            className="risk-dot"
            data-risk={risk}
            role="img"
            aria-label={risk === 'breached' ? 'Password found in a breach' : 'Weak password'}
          />
        ) : null}
      </span>
    </div>
  );
}
