/**
 * Which surface is showing, and what the list is filtered to.
 *
 * Deliberately not a router. There are no URLs in a desktop app, nothing is
 * linkable, and back/forward would be inventing an interaction the design does not
 * have (HO-001 has a sidebar, not history). A store is the honest model: a small
 * closed set of surfaces plus the list query, colocated because the sidebar changes
 * both at once.
 */

import { create } from 'zustand';

import type { GlyphName } from '../components/Glyph';
import type { ItemKindDto, ItemSourceDto, QuickFiltersDto, SortOrderDto } from '../ipc';

/** The surfaces HO-001 covers. */
export type Surface = 'vault' | 'generator' | 'security' | 'settings';

/** No quick filter applied. */
export const NO_FILTERS: QuickFiltersDto = { weak: false, hasTotp: false, shared: false };

export interface NavigationState {
  /** Which pane is showing. */
  surface: Surface;
  /** Which items the list considers. */
  source: ItemSourceDto;
  /** Combinable quick filters (SPEC-V1 §7.1). */
  filters: QuickFiltersDto;
  /** Sort order. Ignored while a search is active, because relevance wins. */
  sort: SortOrderDto;
  /** Fuzzy search text. */
  search: string;
  /** The selected item, or `null`. */
  selectedId: string | null;

  go: (surface: Surface) => void;
  setSource: (source: ItemSourceDto) => void;
  toggleFilter: (filter: keyof QuickFiltersDto) => void;
  /** Replace the whole filter set. Used by the design's "All" chip to clear them. */
  setFilters: (filters: QuickFiltersDto) => void;
  setSort: (sort: SortOrderDto) => void;
  setSearch: (search: string) => void;
  select: (id: string | null) => void;
}

export const useNavigation = create<NavigationState>((set) => ({
  surface: 'vault',
  source: { source: 'all' },
  filters: NO_FILTERS,
  sort: 'recentlyUsed',
  search: '',
  selectedId: null,

  go: (surface) => {
    set({ surface });
  },

  // Changing source clears the selection: the selected item may not be in the new
  // source, and a detail pane showing an item the list no longer contains is the
  // kind of state that looks like a bug even when it is defensible.
  setSource: (source) => {
    set({ source, selectedId: null });
  },

  toggleFilter: (filter) => {
    set((state) => ({
      filters: { ...state.filters, [filter]: !state.filters[filter] },
      selectedId: null,
    }));
  },

  setFilters: (filters) => {
    set({ filters, selectedId: null });
  },

  setSort: (sort) => {
    set({ sort });
  },
  setSearch: (search) => {
    set({ search });
  },
  select: (id) => {
    set({ selectedId: id });
  },
}));

/** Whether a source refers to the same thing, for sidebar selection state. */
export function sameSource(a: ItemSourceDto, b: ItemSourceDto): boolean {
  if (a.source !== b.source) return false;
  if (a.source === 'vault' && b.source === 'vault') return a.id === b.id;
  if (a.source === 'category' && b.source === 'category') return a.kind === b.kind;
  return true;
}

/**
 * The four item categories, in the order and with the labels and glyphs the design's
 * `CATS` list uses (Logins, Secure notes, Cards, Identities).
 */
export const CATEGORIES: readonly { kind: ItemKindDto; label: string; glyph: GlyphName }[] = [
  { kind: 'login', label: 'Logins', glyph: 'login' },
  { kind: 'secureNote', label: 'Secure notes', glyph: 'note' },
  { kind: 'card', label: 'Cards', glyph: 'card' },
  { kind: 'identity', label: 'Identities', glyph: 'identity' },
];

/** The label for a source, for the list column's title (design: `listTitle`). */
export function sourceLabel(source: ItemSourceDto, vaultName?: string): string {
  switch (source.source) {
    case 'all':
      return 'All items';
    case 'favorites':
      return 'Favourites';
    case 'vault':
      return vaultName ?? 'Vault';
    case 'category':
      return CATEGORIES.find((c) => c.kind === source.kind)?.label ?? 'Items';
  }
}
