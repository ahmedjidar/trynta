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
  const custom = items.filter((i) => i.icon.kind === 'custom');
  const ids = custom.map((i) => i.id);
  // Sorted and joined so the key is stable under a re-sort of the list: re-fetching
  // every icon because the user changed the sort order would be pure waste.
  //
  // The revision is in the key, and that is the part that matters. Keyed on ids
  // alone, *replacing* an icon changed nothing about the key — same item, still
  // custom — so the cached bytes were reused and the new icon did not appear until
  // something else evicted them. Every icon write bumps the item revision, so this
  // makes a replacement a different cache entry by construction rather than relying
  // on an invalidation arriving in the right order.
  const key = custom
    .map((i) => `${i.id}@${String(i.revision)}`)
    .sort()
    .join(',');

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
 *
 * @param revision - The item's current revision. Part of the cache key: every icon
 * write bumps it, which is what makes replacing an icon fetch the new bytes rather
 * than reuse the old ones.
 */
export function useItemIcon(
  id: string | null,
  isCustom: boolean,
  revision: number,
): string | undefined {
  const query = useQuery<string | null>({
    // See the note in `useItemIcons`: the revision is what makes a replaced icon a
    // new cache entry instead of a cache hit on the old bytes.
    queryKey: ['item-icon', id ?? '', revision],
    queryFn: () => itemIcon(id ?? ''),
    enabled: id !== null && isCustom,
    ...LOCAL,
  });

  return query.data ?? undefined;
}
