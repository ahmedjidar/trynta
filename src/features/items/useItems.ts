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

import { useNavigation } from '../../app/navigation';
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
export function useClearCache() {
  const client = useQueryClient();
  return useCallback(() => {
    client.clear();
  }, [client]);
}
