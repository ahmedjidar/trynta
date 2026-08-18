/**
 * The bytes behind a `custom` identity tile.
 *
 * The search index carries a flag, not the image — up to 64 KB per item held for the
 * life of the session would put a large vault over SPEC-V1 §9's memory budget for a
 * decoration. So the tile's `data:` URI is fetched by id, and only for the items that
 * actually have one.
 *
 * In a normal vault that is **none of them**: ADD-001 tier 1 resolves almost everything
 * from the bundled set, and the picker is something most people never open. The hook
 * short-circuits to an empty map when no visible row is `custom`, so the common case
 * costs one `useMemo` and no IPC at all.
 */

import { useQuery } from '@tanstack/react-query';

import { itemIcon } from '../../ipc';
import type { ItemSummaryDto } from '../../ipc';

/** Id to `data:` URI, for the rows that have a custom icon. */
export type IconSources = Readonly<Record<string, string>>;

const EMPTY: IconSources = Object.freeze({});

/** Shared options, matching the rest of the query layer: nothing survives a lock. */
const LOCAL = {
  gcTime: 0,
  staleTime: 0,
  refetchOnWindowFocus: false,
  retry: false,
} as const;

/**
 * Fetch the `data:` URIs for every item in `items` whose icon is custom.
 *
 * @param items - The rows being rendered. Only the custom ones are fetched.
 */
export function useItemIcons(items: readonly ItemSummaryDto[]): IconSources {
  const ids = items.filter((i) => i.icon.kind === 'custom').map((i) => i.id);
  // Sorted and joined so the key is stable under a re-sort of the list: re-fetching
  // every icon because the user changed the sort order would be pure waste.
  const key = [...ids].sort().join(',');

  const query = useQuery<IconSources>({
    queryKey: ['item-icons', key],
    queryFn: async () => {
      const entries = await Promise.all(ids.map(async (id) => [id, await itemIcon(id)] as const));
      return Object.fromEntries(
        entries.filter((e): e is readonly [string, string] => e[1] !== null),
      );
    },
    enabled: ids.length > 0,
    ...LOCAL,
  });

  return query.data ?? EMPTY;
}

/**
 * Fetch one item's custom icon, for the detail pane.
 *
 * Separate from {@link useItemIcons} because the detail pane renders a 56px tile for one
 * item and should not wait on a batch keyed to the whole list.
 */
export function useItemIcon(id: string | null, isCustom: boolean): string | undefined {
  const query = useQuery<string | null>({
    queryKey: ['item-icon', id ?? ''],
    queryFn: () => itemIcon(id ?? ''),
    enabled: id !== null && isCustom,
    ...LOCAL,
  });

  return query.data ?? undefined;
}
