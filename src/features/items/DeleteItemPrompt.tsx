/**
 * The confirmation in front of deleting an item.
 *
 * There was no way to delete an item at all — every other verb existed, and the one
 * that removes a row someone no longer wants did not. `item_delete` had been in Rust
 * and in the IPC surface the whole time with nothing calling it.
 *
 * It is back behind two gates, and the reason for two is the threat this has to
 * survive: an unlocked vault on an unattended desk. Neither gate is difficulty for
 * its own sake.
 *
 * - **The master password** proves the person deleting is the person who owns the
 *   vault, not whoever walked past while it was open.
 * - **Typing the item's name** proves they know *which* item they are deleting. A
 *   password alone is muscle memory; a password plus a name that has to be read off
 *   the item in front of you is not something done by accident.
 *
 * Both are checked in `item_delete` in Rust. The name box below disables the button
 * until it matches, which is a courtesy — it saves a round trip and shows the user
 * they have it right — and not the enforcement. Enforcement in the webview would be
 * enforcement anyone calling the command directly skips.
 */

import { useState } from 'react';

import { Button } from '../../components/Button';
import { Input } from '../../components/Bits';
import { Glyph } from '../../components/Glyph';
import { Overlay } from '../../components/Overlay';
import { itemDelete } from '../../ipc';

export interface DeleteItemPromptProps {
  /** The item being deleted. */
  id: string;
  /** Its title, which the user has to type back. */
  title: string;
  /** Deleted; the caller should navigate away and refresh. */
  onDeleted: () => void;
  /** Abandon the deletion. */
  onCancel: () => void;
}

/**
 * Confirm and delete one item.
 *
 * @param props - See {@link DeleteItemPromptProps}.
 */
export function DeleteItemPrompt({ id, title, onDeleted, onCancel }: DeleteItemPromptProps) {
  const [password, setPassword] = useState('');
  const [typed, setTyped] = useState('');
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  // The same comparison Rust makes, so the button does not enable on something the
  // command will refuse — and does not stay disabled on something it would accept.
  const matches = typed.trim().toLowerCase() === title.trim().toLowerCase();
  const ready = password !== '' && matches && !busy;

  function submit() {
    if (!ready) return;
    setBusy(true);
    itemDelete(id, password, typed).then(
      () => {
        setPassword('');
        setBusy(false);
        onDeleted();
      },
      () => {
        setBusy(false);
        setFailed(true);
        setPassword('');
      },
    );
  }

  return (
    <Overlay onDismiss={onCancel} label={`Delete ${title}`} placement="centre">
      <div className="bg-surface-panel shadow-sheet w-[440px] rounded-xl p-6">
        <div className="flex items-start gap-3">
          {/* The tone is on the wrapper, not the glyph: `Glyph` takes a name, one of
            three sizes and a label, and widening it would be inventing a design
            control. Lucide strokes in `currentColor`, so a coloured parent is all it
            needs. */}
          <span
            className="bg-status-danger-subtle grid h-9 w-9 shrink-0 place-items-center rounded-full"
            data-tone="danger"
            aria-hidden="true"
          >
            <Glyph name="warning" size={16} />
          </span>
          <div className="min-w-0">
            <h2 className="text-title text-text-primary font-bold tracking-tight">
              Delete {title}?
            </h2>
            <p className="text-body text-text-secondary mt-2">
              This removes the item and everything in it — password, one-time code, custom fields
              and notes. Nothing else in your vault changes.
            </p>
          </div>
        </div>

        <form
          className="mt-5"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <label
            className="text-caption text-text-secondary block font-semibold"
            htmlFor="confirm-title"
          >
            Type <span className="text-text-primary font-bold">{title}</span> to confirm
          </label>
          <Input
            id="confirm-title"
            autoFocus
            aria-label={`Type ${title} to confirm`}
            className="mt-1.5 h-10 w-full"
            value={typed}
            placeholder={title}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setTyped(event.target.value);
            }}
          />

          <label
            className="text-caption text-text-secondary mt-4 block font-semibold"
            htmlFor="confirm-password"
          >
            Master password
          </label>
          <Input
            id="confirm-password"
            type="password"
            aria-label="Master password"
            aria-invalid={failed}
            className="mt-1.5 h-10 w-full"
            value={password}
            placeholder="Master password"
            autoComplete="off"
            onChange={(event) => {
              setPassword(event.target.value);
              if (failed) setFailed(false);
            }}
          />

          {/* One message for both gates, because Rust returns one refusal for both and
            saying which half failed would tell an onlooker which half they already
            have. */}
          {failed ? (
            <p className="text-caption mt-2" data-tone="danger" role="alert">
              That did not check out. Confirm the name and your master password.
            </p>
          ) : null}

          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={onCancel}>
              Keep it
            </Button>
            <Button type="submit" disabled={!ready}>
              {busy ? 'Deleting…' : 'Delete'}
            </Button>
          </div>
        </form>
      </div>
    </Overlay>
  );
}
