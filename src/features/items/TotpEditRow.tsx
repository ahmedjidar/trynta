/**
 * The one-time-code row while an item is being edited.
 *
 * The detail view knows *whether* an item has a one-time code — `has_totp` is
 * metadata — but never the configuration itself, because the seed is a secret and
 * secrets are fetched one at a time on explicit user action (SPEC-V1 §6). So this
 * row cannot pre-fill anything, and it does not pretend to: an item that already
 * has a code shows that it has one, and offers to replace or remove it.
 *
 * ## Three states, not two
 *
 * `pending` is `undefined` for "untouched", `null` for "remove it", and a
 * configuration for "use this one". Collapsing that to a nullable would make
 * "leave the existing code alone" and "delete the existing code" the same value,
 * and the save path would have to guess which the user meant.
 */

import { useState } from 'react';

import { CopyAction } from '../../components/Bits';
import { FieldLabel, GroupedRow } from '../../components/GroupedList';
import { TotpField } from './TotpField';
import type { TotpConfigInput } from '../../ipc';

export interface TotpEditRowProps {
  /** Whether the saved item already carries a configuration. */
  configured: boolean;
  /** The pending change: `undefined` untouched, `null` remove, otherwise replace. */
  pending: TotpConfigInput | null | undefined;
  /** Called with the new pending value. */
  onChange: (next: TotpConfigInput | null | undefined) => void;
}

/**
 * Add, replace or remove an item's one-time-code setup.
 *
 * @param props - See {@link TotpEditRowProps}.
 */
export function TotpEditRow({ configured, pending, onChange }: TotpEditRowProps) {
  // Opening the entry box on an item that already has a code is a deliberate act,
  // so it is a separate step rather than the default: an editor that showed an
  // empty box beside a working code would read as though the code had been lost.
  const [replacing, setReplacing] = useState(false);

  if (configured && pending === undefined && !replacing) {
    return (
      <GroupedRow className="h-[52px]">
        <FieldLabel>One-time code</FieldLabel>
        <span className="text-body text-text-primary min-w-0 flex-1 font-semibold">Set up</span>
        <CopyAction
          className="h-[30px] rounded-md px-[11px]"
          onClick={() => {
            setReplacing(true);
          }}
        >
          Replace
        </CopyAction>
        <CopyAction
          className="h-[30px] rounded-md px-[11px]"
          onClick={() => {
            onChange(null);
          }}
        >
          Remove
        </CopyAction>
      </GroupedRow>
    );
  }

  if (configured && pending === null) {
    return (
      <GroupedRow className="h-[52px]">
        <FieldLabel>One-time code</FieldLabel>
        <span className="text-body min-w-0 flex-1" data-tone="danger">
          Will be removed when you save
        </span>
        <CopyAction
          className="h-[30px] rounded-md px-[11px]"
          onClick={() => {
            onChange(undefined);
            setReplacing(false);
          }}
        >
          Keep it
        </CopyAction>
      </GroupedRow>
    );
  }

  return (
    <TotpField
      value={pending ?? null}
      onChange={(next) => {
        // Clearing the box on an item that has no saved code means "still none",
        // which is `undefined` — not `null`, which would queue a pointless removal.
        if (next === null && !configured) {
          onChange(undefined);
          return;
        }
        onChange(next);
      }}
    />
  );
}
