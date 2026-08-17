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
 */

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback } from 'react';

import { useNavigation } from '../../app/navigation';
import { itemsList, vaultsList } from '../../ipc';
import type { ItemSummaryDto, VaultSummaryDto } from '../../ipc';

/** Query keys, in one place so an invalidation cannot miss one. */
export const keys = {
  items: (source: unknown, filters: unknown, sort: unknown, search: string) =>
    ['items', source, filters, sort, search] as const,
  vaults: ['vaults'] as const,
};

/** Shared options: nothing about a local vault benefits from a stale window. */
const LOCAL = {
  gcTime: 0,
  staleTime: 0,
  refetchOnWindowFocus: false,
  retry: false,
} as const;

/** The list for the current source, filters, sort and search. */
export function useItems() {
  const source = useNavigation((s) => s.source);
  const filters = useNavigation((s) => s.filters);
  const sort = useNavigation((s) => s.sort);
  const search = useNavigation((s) => s.search);

  return useQuery<ItemSummaryDto[]>({
    queryKey: keys.items(source, filters, sort, search),
    queryFn: () => itemsList({ source, filters, sort, search }),
    ...LOCAL,
  });
}

/** Every vault, for the sidebar. */
export function useVaults() {
  return useQuery<VaultSummaryDto[]>({
    queryKey: keys.vaults,
    queryFn: () => vaultsList(),
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
