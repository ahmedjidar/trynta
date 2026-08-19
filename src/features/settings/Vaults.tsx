// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Vault management: create, rename, recolour, delete.
 *
 * // UNSTYLED: awaiting handoff vault-management
 *
 * HO-002 draws three vaults in the sidebar and no way to make a fourth, so every
 * vault the product could hold was one of the three the mock happened to contain.
 * `vault_add`, `vault_rename`, `vault_set_color` and `vault_delete` have existed on
 * the IPC surface since run 2 with nothing calling any of them.
 *
 * This is built from the existing component vocabulary and the token layer only —
 * no new colours, spacing or type — so that replacing it with a real handoff is a
 * swap rather than an unpick. It is marked above so nobody mistakes it for drawn.
 *
 * ## Deleting a vault asks where its items go
 *
 * `vault_delete` takes a destination, because the alternative is deciding on the
 * user's behalf whether a vault's contents are rubbish. §4.2's rule that the last
 * vault cannot be deleted is enforced in Rust; this hides the action rather than
 * offering it and reporting `lastVaultRemaining`, which would be a worse way to
 * learn the same thing.
 */

import { useState } from 'react';

import { Button } from '../../components/Button';
import { CopyAction, Input } from '../../components/Bits';
import { GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { IpcError, vaultAdd, vaultDelete, vaultRename, vaultSetColor } from '../../ipc';
import type { VaultSummaryDto } from '../../ipc';

/**
 * The accent tokens a vault may use.
 *
 * Token *names*, never values — a colour crossing IPC would be a hardcoded colour
 * outside the token layer, which CLAUDE.md §3 bans and `vault_set_color` rejects.
 */
const ACCENTS = [
  'vault.accent.1',
  'vault.accent.2',
  'vault.accent.3',
  'vault.accent.4',
  'vault.accent.5',
  'vault.accent.6',
  'vault.accent.7',
] as const;

export interface VaultsProps {
  /** Every vault, from `vaults_list`. */
  vaults: readonly VaultSummaryDto[];
  /** Reload the list and the item counts after a change. */
  onChanged: () => void;
  /** Report success to the toast. */
  onDone: (what: string) => void;
  /** Report failure to the toast. */
  onFailed: (message: string) => void;
  /** Leave this surface. */
  onBack: () => void;
}

/**
 * Create and manage vaults.
 *
 * @param props - See {@link VaultsProps}.
 */
export function Vaults({ vaults, onChanged, onDone, onFailed, onBack }: VaultsProps) {
  const [name, setName] = useState('');
  const [accent, setAccent] = useState<string>(ACCENTS[0]);
  const [busy, setBusy] = useState(false);
  const [renaming, setRenaming] = useState<{ id: string; value: string } | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);

  function fail(error: unknown, fallback: string) {
    const message =
      error instanceof IpcError && error.error.kind === 'lastVaultRemaining'
        ? 'A vault has to exist, so the last one cannot be deleted.'
        : fallback;
    onFailed(message);
  }

  function create() {
    const trimmed = name.trim();
    if (trimmed === '' || busy) return;
    setBusy(true);
    vaultAdd(trimmed, accent).then(
      () => {
        setBusy(false);
        setName('');
        onChanged();
        onDone(`Created ${trimmed}`);
      },
      (error: unknown) => {
        setBusy(false);
        fail(error, 'That vault could not be created.');
      },
    );
  }

  function commitRename() {
    if (renaming === null) return;
    const trimmed = renaming.value.trim();
    const original = vaults.find((v) => v.id === renaming.id)?.name ?? '';
    if (trimmed === '' || trimmed === original) {
      setRenaming(null);
      return;
    }
    vaultRename(renaming.id, trimmed).then(
      () => {
        setRenaming(null);
        onChanged();
        onDone(`Renamed to ${trimmed}`);
      },
      (error: unknown) => {
        setRenaming(null);
        fail(error, 'That vault could not be renamed.');
      },
    );
  }

  function recolour(id: string, token: string) {
    vaultSetColor(id, token).then(
      () => {
        onChanged();
      },
      (error: unknown) => {
        fail(error, 'That colour could not be applied.');
      },
    );
  }

  function remove(id: string) {
    // Items move rather than vanish. The destination is the first other vault,
    // which is the only choice that needs no question when there are two; with
    // more, a real handoff should let the user pick.
    const destination = vaults.find((v) => v.id !== id);
    if (destination === undefined) return;
    const removed = vaults.find((v) => v.id === id)?.name ?? 'that vault';
    vaultDelete(id, destination.id).then(
      () => {
        setConfirming(null);
        onChanged();
        onDone(`Deleted ${removed}; its items moved to ${destination.name}`);
      },
      (error: unknown) => {
        setConfirming(null);
        fail(error, 'That vault could not be deleted.');
      },
    );
  }

  return (
    <section
      data-scroll-pane
      className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
      aria-label="Vaults"
    >
      <div className="mx-auto w-full max-w-[var(--measure-pane)] px-10 pt-8 pb-12">
        <header className="flex items-center gap-3">
          <Button variant="outline" onClick={onBack}>
            Back
          </Button>
          <h1 className="text-display text-text-primary font-extrabold tracking-tight">Vaults</h1>
        </header>
        <p className="text-body text-text-secondary mt-2">
          A vault is a group of items with its own colour. Make as many as you like.
        </p>

        <SectionLabel className="mt-8">New vault</SectionLabel>
        <GroupedList className="mt-2">
          <GroupedRow className="h-[52px]">
            <Input
              aria-label="New vault name"
              className="flex-1"
              value={name}
              placeholder="Household, Side project, Archive…"
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => {
                setName(event.target.value);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  create();
                }
              }}
            />
            <Button onClick={create} disabled={name.trim() === '' || busy}>
              {busy ? 'Creating…' : 'Create'}
            </Button>
          </GroupedRow>
          <GroupedRow className="h-[52px]">
            <span className="text-caption text-text-muted w-[92px] shrink-0 font-bold uppercase">
              Colour
            </span>
            <div className="flex flex-1 gap-2" role="radiogroup" aria-label="Vault colour">
              {ACCENTS.map((token, index) => (
                <button
                  key={token}
                  type="button"
                  role="radio"
                  aria-checked={accent === token}
                  aria-label={`Colour ${String(index + 1)}`}
                  data-focus-ring
                  className="duration-quick shrink-0 rounded-full p-1 transition-colors"
                  data-selected={accent === token ? 'true' : undefined}
                  onClick={() => {
                    setAccent(token);
                  }}
                >
                  <span
                    className="swatch block h-3.5 w-3.5 shrink-0 rounded-full"
                    data-accent={token}
                    aria-hidden="true"
                  />
                </button>
              ))}
            </div>
          </GroupedRow>
        </GroupedList>

        <SectionLabel className="mt-8">
          {vaults.length === 1 ? '1 vault' : `${String(vaults.length)} vaults`}
        </SectionLabel>
        <GroupedList className="mt-2">
          {vaults.map((vault) => {
            return (
              <GroupedRow key={vault.id} className="h-auto min-h-[60px] flex-wrap gap-2 py-2.5">
                <span
                  className="swatch block h-2.5 w-2.5 shrink-0 rounded-full"
                  data-accent={vault.colorToken}
                  aria-hidden="true"
                />
                {renaming?.id === vault.id ? (
                  <Input
                    aria-label={`Rename ${vault.name}`}
                    className="min-w-0 flex-1"
                    value={renaming.value}
                    autoFocus
                    onChange={(event) => {
                      setRenaming({ id: vault.id, value: event.target.value });
                    }}
                    onBlur={commitRename}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault();
                        commitRename();
                      }
                      if (event.key === 'Escape') setRenaming(null);
                    }}
                  />
                ) : (
                  <div className="min-w-0 flex-1">
                    <div className="text-body text-text-primary font-semibold">{vault.name}</div>
                    <div className="text-caption text-text-muted">
                      {vault.itemCount === 1 ? '1 item' : `${String(vault.itemCount)} items`}
                    </div>
                  </div>
                )}

                <div className="flex shrink-0 gap-1">
                  {ACCENTS.map((token, i) => (
                    <button
                      key={token}
                      type="button"
                      aria-label={`Set ${vault.name} to colour ${String(i + 1)}`}
                      aria-pressed={vault.colorToken === token}
                      data-focus-ring
                      className="rounded-full p-0.5"
                      onClick={() => {
                        recolour(vault.id, token);
                      }}
                    >
                      <span
                        className="swatch block h-3.5 w-3.5 shrink-0 rounded-full"
                        data-accent={token}
                        data-selected={vault.colorToken === token ? 'true' : undefined}
                        aria-hidden="true"
                      />
                    </button>
                  ))}
                </div>

                <CopyAction
                  className="h-[30px] shrink-0 rounded-md px-[11px]"
                  onClick={() => {
                    setRenaming({ id: vault.id, value: vault.name });
                  }}
                >
                  Rename
                </CopyAction>

                {vaults.length > 1 ? (
                  confirming === vault.id ? (
                    <CopyAction
                      className="h-[30px] shrink-0 rounded-md px-[11px]"
                      data-tone="danger"
                      onClick={() => {
                        remove(vault.id);
                      }}
                    >
                      Move items and delete
                    </CopyAction>
                  ) : (
                    <CopyAction
                      className="h-[30px] shrink-0 rounded-md px-[11px]"
                      onClick={() => {
                        setConfirming(vault.id);
                      }}
                    >
                      Delete
                    </CopyAction>
                  )
                ) : null}
              </GroupedRow>
            );
          })}
        </GroupedList>

        <p className="text-caption text-text-muted mt-4 max-w-[68ch] leading-relaxed">
          Deleting a vault moves its items into another one — nothing is thrown away. A vault has to
          exist, so the last one cannot be deleted.
        </p>
      </div>
    </section>
  );
}
