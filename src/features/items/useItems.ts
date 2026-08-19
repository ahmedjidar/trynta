/**
 * Item and vault data over IPC.
 *
 * TanStack Query for everything async across the boundary (CLAUDE.md §2). The
 * important settings are not performance tuning, they are lock behaviour:
 *
 * - **`gcTime: 0`.** A cached item list survives a lock otherwise. §4.9 says locking
 *   *"tears down decrypted caches"*, and a query cache holding decrypted titles is a
 *   decrypted cache — it does not stop being one because it holds no passwords.
 * - **No `refetchOnWindowFocus`.** Focus fires on every alt-tab, and each refetch is
 *   a full index query. Nothing here changes without the app changing it.
 * - **`unlocked` gates every query.** Without it these fire during the locked window on
 *   every boot, fail, and — with `retry: false` — hold that rejection until something
 *   clears the cache. The visible symptom was an unlocked vault showing "Nothing here
 *   yet", which is a lie about the contents rather than a loading glitch.
 */

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback } from 'react';

import { NO_FILTERS, useNavigation } from '../../app/navigation';
import { itemGet, itemsList, vaultsList } from '../../ipc';
import type { ItemDetailDto, ItemSummaryDto, VaultSummaryDto } from '../../ipc';

/** Query keys, in one place so an invalidation cannot miss one. */
export const keys = {
  items: (source: unknown, filters: unknown, sort: unknown, search: string) =>
    ['items', source, filters, sort, search] as const,
  vaults: ['vaults'] as const,
  detail: (id: string) => ['item', id] as const,
};

/** Shared options: nothing about a local vault benefits from a stale window. */
const LOCAL = {
  gcTime: 0,
  staleTime: 0,
  refetchOnWindowFocus: false,
  retry: false,
} as const;

/**
 * The list for the current source, filters, sort and search.
 *
 * @param unlocked - Whether the vault is open. False keeps the query idle rather than
 * letting it fail against a locked vault.
 */
export function useItems(unlocked = true) {
  const source = useNavigation((s) => s.source);
  const filters = useNavigation((s) => s.filters);
  const sort = useNavigation((s) => s.sort);
  const search = useNavigation((s) => s.search);

  return useQuery<ItemSummaryDto[]>({
    queryKey: keys.items(source, filters, sort, search),
    queryFn: () => itemsList({ source, filters, sort, search }),
    enabled: unlocked,
    ...LOCAL,
  });
}

/**
 * The whole vault, unfiltered, for the sidebar's counts.
 *
 * Separate from {@link useItems} deliberately: that one is keyed on the current source,
 * filters and search, so counting from it makes "All items" report the size of whatever
 * the user is currently looking at — "Cards 1, Logins 0" while a card is selected. The
 * counts are a property of the vault, not of the view.
 *
 * It runs against the Rust-side index, so the extra call is a filter over memory rather
 * than a decrypt.
 */
export function useAllItems(unlocked = true) {
  return useQuery<ItemSummaryDto[]>({
    queryKey: keys.items({ source: 'all' }, NO_FILTERS, 'alphabetical', ''),
    queryFn: () =>
      itemsList({
        source: { source: 'all' },
        filters: NO_FILTERS,
        sort: 'alphabetical',
        search: '',
      }),
    enabled: unlocked,
    ...LOCAL,
  });
}

/** Every vault, for the sidebar. */
export function useVaults(unlocked = true) {
  return useQuery<VaultSummaryDto[]>({
    queryKey: keys.vaults,
    queryFn: () => vaultsList(),
    enabled: unlocked,
    ...LOCAL,
  });
}

/**
 * One item's metadata and secret presence.
 *
 * Disabled while nothing is selected, so selecting and deselecting does not leave a
 * stale detail query in flight. Returns presence flags, never a secret value.
 */
export function useItemDetail(id: string | null) {
  return useQuery<ItemDetailDto>({
    queryKey: keys.detail(id ?? ''),
    queryFn: () => itemGet(id ?? ''),
    enabled: id !== null,
    ...LOCAL,
  });
}

/**
 * Drop every cached query.
 *
 * Called on lock. `clear()` rather than `invalidateQueries`: invalidation marks data
 * stale and keeps it, which is the opposite of what §4.9 requires.
 */
/**
 * Refetch everything, without emptying the screen first.
 *
 * The difference from {@link useClearCache} is what happens *during* the refetch.
 * `clear()` removes the cached data, so every component reading it renders its
 * empty state for a frame or two — the detail pane finds no summary for the
 * selected id, decides nothing is selected, and shows "Select an item". Favouriting
 * an item made the pane you were looking at blink out and come back, and an icon
 * change did the same.
 *
 * `invalidateQueries` marks the data stale and refetches while continuing to serve
 * the previous value, so the pane stays put and the new value replaces the old one
 * when it lands. That is what every edit wants.
 *
 * `clear()` is still right where the data is genuinely gone — a lock, or a restore
 * that rewrote the vault file underneath us — and {@link useClearCache} remains for
 * those.
 */
export function useRefresh() {
  const client = useQueryClient();
  return useCallback(() => {
    void client.invalidateQueries();
  }, [client]);
}

export function useClearCache() {
  const client = useQueryClient();
  return useCallback(() => {
    client.clear();
  }, [client]);
}
