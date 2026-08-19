// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Item list column — SPEC-V1 §7.1.
 *
 * The design gives every row `tabIndex={0}` inside its listbox, so each row is its own tab
 * stop. This keeps that visual treatment and moves the tab stop to the list, with
 * `aria-activedescendant` carrying the selection: a 5,000-item vault must not cost 5,000
 * tab presses to walk past, and binding the arrow keys to the list rather than to `window`
 * is what stops Down moving the selection while the user is typing in the search field.
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

import { NO_FILTERS, sourceLabel, useNavigation } from '../../app/navigation';
import { Badge, Chip } from '../../components/Bits';
import { Glyph } from '../../components/Glyph';
import { IdentityTile } from '../../components/IdentityTile';
import { useItemIcons } from './useItemIcons';
import type { IconSources } from './useItemIcons';
import { useThemeStore } from '../../theme/store';
import { Spacer } from '../../components/Spacer';
import { cn } from '../../lib/cn';
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
  /** Vault names by id, so a vault source can title the column with its own name. */
  vaultNames: Readonly<Record<string, string>>;
  /** Copy the selected item's primary secret, in Rust. */
  onCopy: (id: string) => void;
  /** Open the new-item sheet. */
  onNew: () => void;
  /** The platform's modifier label, for the keyboard hints. Never hardcoded (§8). */
  modifierKey: string;
}

export function ItemList({ items, risks, vaultNames, onCopy, onNew, modifierKey }: ItemListProps) {
  const selectedId = useNavigation((s) => s.selectedId);
  const source = useNavigation((s) => s.source);
  const setFilters = useNavigation((s) => s.setFilters);
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

  // Only the rows with a custom icon cost anything here; in most vaults that is none.
  const iconSources = useItemIcons(items);
  const resolved = useThemeStore((s) => s.resolved);

  const currentSort = SORTS.find((s) => s.value === sort) ?? SORTS[0];
  const anyFilter = filters.weak || filters.hasTotp || filters.shared;

  return (
    <section
      className="border-hairline bg-surface-raised flex w-[clamp(var(--width-list),28%,440px)] shrink-0 flex-col border-r"
      aria-label="Items"
    >
      <header className="border-hairline flex h-11 shrink-0 items-center gap-2 border-b pr-3 pl-4">
        {/* The column title is the selected source's own name, not a fixed word:
            "All items", "Logins", or the vault's name. */}
        <h1 className="text-heading tracking-title font-bold">
          {sourceLabel(source, source.source === 'vault' ? vaultNames[source.id] : undefined)}
        </h1>
        <span className="text-caption text-text-muted tabular-nums">
          {items.length === 1 ? '1 item' : `${String(items.length)} items`}
        </span>
        <div className="flex-1" />
        <button
          type="button"
          data-focus-ring
          className="text-micro text-text-secondary duration-hover hover:bg-surface-hover hover:text-text-primary flex h-6 items-center gap-1 rounded-full px-2 font-semibold transition-colors"
          onClick={() => {
            const at = SORTS.findIndex((s) => s.value === sort);
            const next = SORTS[(at + 1) % SORTS.length];
            if (next) setSort(next.value);
          }}
          aria-label={`Sort: ${currentSort?.label ?? ''}. Change sort order`}
        >
          {currentSort?.label}
          <Glyph name="sort" size={12} />
        </button>
        <button
          type="button"
          data-focus-ring
          className="bg-accent text-text-on-accent shadow-add duration-instant flex h-6 w-6 shrink-0 items-center justify-center rounded-full transition-transform active:scale-[.92]"
          onClick={onNew}
          aria-label="New item"
        >
          <Glyph name="add" />
        </button>
      </header>

      {/* Quick filters (§7.1). `shared` is V2 and is deliberately absent rather than
          present and inert — "never a toggle that does nothing" (§7.5). */}
      <div
        className="border-hairline flex h-10 shrink-0 items-center gap-1.5 border-b px-3"
        role="group"
        aria-label="Quick filters"
      >
        {/* The design's bar is All / Weak / Has 2FA / Shared, and its own logic makes
            them exclusive. §7.1 says "combinable" and the spec wins on behaviour, so the
            two real ones toggle and All clears them. `Shared` is V2 and is absent rather
            than inert — "never a toggle that does nothing" (§7.5). */}
        <Chip
          selected={!anyFilter}
          onClick={() => {
            setFilters(NO_FILTERS);
          }}
        >
          All
        </Chip>
        <Chip
          selected={filters.weak}
          onClick={() => {
            toggleFilter('weak');
          }}
        >
          Weak
        </Chip>
        <Chip
          selected={filters.hasTotp}
          onClick={() => {
            toggleFilter('hasTotp');
          }}
        >
          Has 2FA
        </Chip>
      </div>

      <div
        data-scroll-pane
        className="flex-1 overflow-y-auto px-2 pt-2 pb-4"
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
          <p className="text-caption text-text-muted px-6 py-14 text-center">
            {search !== ''
              ? `No items match “${search}”.`
              : anyFilter
                ? 'Nothing matches this filter.'
                : 'Nothing here yet.'}
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
                iconSources={iconSources}
                theme={resolved}
                onSelect={() => {
                  select(item.id);
                }}
              />
            ))}
            <Spacer height={(items.length - window_.end) * rowHeight} />
          </>
        )}
      </div>

      {/* The modifier in these hints resolves from the platform rather than being typed
          (SPEC-V1 §8). */}
      <footer className="border-hairline text-micro text-text-muted flex h-8 shrink-0 items-center gap-3.5 border-t px-4">
        <span>↑↓ Navigate</span>
        <span>⏎ Open</span>
        <span>{modifierKey}C Copy</span>
      </footer>
    </section>
  );
}

interface ItemRowProps {
  item: ItemSummaryDto;
  risk: 'breached' | 'weak' | undefined;
  selected: boolean;
  /** `data:` URIs for the rows whose icon the user supplied. */
  iconSources: IconSources;
  /** Resolved theme, for brands that ship a light/dark pair. */
  theme: 'light' | 'dark';
  onSelect: () => void;
}

function ItemRow({ item, risk, selected, iconSources, theme, onSelect }: ItemRowProps) {
  return (
    <div
      id={`item-${item.id}`}
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      className={cn(
        // `item-row` is what the windowing effect measures a real row's height from: the
        // token cannot be read back through `getComputedStyle`, which resolves a custom
        // property to its specified value rather than to pixels.
        'item-row duration-hover flex h-[var(--row-h)] shrink-0 cursor-pointer items-center gap-3 rounded-lg px-3 transition-colors',
        selected ? 'bg-surface-selected' : 'hover:bg-surface-hover',
      )}
    >
      <IdentityTile
        icon={item.icon}
        title={item.title}
        customSrc={iconSources[item.id]}
        theme={theme}
      />
      <div className="min-w-0 flex-1">
        <div
          className={cn(
            'text-body truncate tracking-tight',
            selected ? 'font-bold' : 'font-medium',
          )}
        >
          {item.title}
        </div>
        <div className="text-chip text-text-muted truncate">{item.subtitle ?? ''}</div>
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        {item.hasTotp ? (
          <Badge tone="accent" size="sm">
            2FA
          </Badge>
        ) : null}
        {risk ? (
          <span
            className="dot h-1.5 w-1.5 rounded-full"
            data-tone={risk === 'breached' ? 'danger' : 'warning'}
            role="img"
            aria-label={risk === 'breached' ? 'Password found in a breach' : 'Weak password'}
          />
        ) : null}
      </div>
    </div>
  );
}
