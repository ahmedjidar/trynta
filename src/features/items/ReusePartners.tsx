/**
 * The other items sharing this item's password.
 *
 * §7.4 asks the report to show reuse *groups* "so the user sees what else is
 * affected". The report did that; the item did not — so someone looking at a
 * flagged login was told it was reused and had to go somewhere else to find out
 * what it was reused with. That is the one piece of information that makes the
 * warning actionable, because changing this password only helps if you also know
 * the other three places it is doing the same job.
 *
 * ## Where the data comes from, and what that costs
 *
 * The reuse grouping is the security report's, read from the cache rather than
 * recomputed: grouping needs every password in the vault decrypted, and doing that
 * because a detail pane is open would decrypt the whole vault on every click. So
 * this renders nothing until the user has opened the security report at least once
 * in this session, and says nothing rather than implying "not reused" — which is
 * the same rule as §7.4's *offline is "not checked", never "safe"*.
 */

import { useCachedSecurityReport } from '../security/useSecurity';
import { FieldLabel, GroupedRow } from '../../components/GroupedList';
import type { ItemSummaryDto } from '../../ipc';

export interface ReusePartnersProps {
  /** The item being viewed. */
  itemId: string;
  /** List rows, to turn the group's ids into titles. */
  items: readonly ItemSummaryDto[];
  /** Open one of the partners. */
  onOpen: (id: string) => void;
}

/**
 * A row naming the other items that share this password, or nothing.
 *
 * @param props - See {@link ReusePartnersProps}.
 */
export function ReusePartners({ itemId, items, onOpen }: ReusePartnersProps) {
  const { data: report } = useCachedSecurityReport();
  if (report === undefined) return null;

  const group = report.reuseGroups.find((g) => g.itemIds.includes(itemId));
  if (group === undefined) return null;

  const partners = group.itemIds
    .filter((id) => id !== itemId)
    .map((id) => ({ id, title: items.find((i) => i.id === id)?.title }))
    // An id with no matching row means the list is filtered to another vault. Its
    // title is not ours to invent, so it is dropped rather than shown as "Unknown".
    .filter((p): p is { id: string; title: string } => p.title !== undefined);

  if (partners.length === 0) return null;

  return (
    <GroupedRow className="h-auto min-h-[52px] py-2">
      <FieldLabel>Also used by</FieldLabel>
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
        {partners.map((partner) => (
          <button
            key={partner.id}
            type="button"
            className="text-chip border-strong bg-surface-raised text-text-secondary hover:text-text-primary duration-quick h-[26px] rounded-md border px-2 font-semibold transition-colors"
            data-focus-ring
            onClick={() => {
              onOpen(partner.id);
            }}
          >
            {partner.title}
          </button>
        ))}
      </div>
    </GroupedRow>
  );
}
