/**
 * Backup and restore — SPEC-V1 §7.8.
 *
 * The design does not draw this surface: its settings list has an "Import from another
 * manager" row and nothing for backups. It therefore reuses the settings vocabulary the
 * design does define — section labels, grouped rows, the raised card for a statement —
 * rather than inventing a layout. Raised for the next design pass in
 * `handoffs/MANIFEST.md`.
 *
 * ## The three restore modes are the whole point of this screen
 *
 * `RestoreModeDto` is `fresh | merge | replace`, and they are genuinely different
 * operations. **Replace destroys a vault belonging to a different account** — nothing
 * in the container decrypts under the master password of the vault that is here, so
 * merging is not possible. A screen that called all three "restore" and got on with it
 * would be the worst copy in this product, so replace has its own confirmation and its
 * own sentence saying what it will do.
 *
 * ## The passphrase never becomes the master password
 *
 * §7.8 gives a backup its own passphrase. The form says so, because a user who types
 * their master password here would produce a container whose compromise is equivalent
 * to the vault's.
 */

import { useState } from 'react';

import { Button } from '../../components/Button';
import { Input } from '../../components/Bits';
import { Card, GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { Glyph } from '../../components/Glyph';
import { IpcError, backupExport, backupPreview, backupRestore } from '../../ipc';
import type { BackupPreviewDto } from '../../ipc';

/** §7.8's floor, mirrored from `commands/backup.rs` so the form can gate locally. */
const MIN_PASSPHRASE = 12;

export interface BackupProps {
  /** Back to the settings list. */
  onBack: () => void;
  /** Report success to the toast. */
  onDone: (message: string) => void;
  onFailed: (message: string) => void;
  /** A restore that replaces the vault locks it; the shell has to re-read the state. */
  onVaultReplaced: () => void;
}

export function Backup({ onBack, onDone, onFailed, onVaultReplaced }: BackupProps) {
  const [exportPass, setExportPass] = useState('');
  const [restorePass, setRestorePass] = useState('');
  const [preview, setPreview] = useState<BackupPreviewDto | null>(null);
  const [busy, setBusy] = useState<'export' | 'preview' | 'restore' | null>(null);

  const exportReady = exportPass.length >= MIN_PASSPHRASE;
  const restoreReady = restorePass.length >= MIN_PASSPHRASE;

  function runExport() {
    setBusy('export');
    backupExport(exportPass).then(
      (summary) => {
        setBusy(null);
        if (summary === null) return; // cancelled, not an error
        setExportPass('');
        onDone(`Backed up ${String(summary.items)} items`);
      },
      (cause: unknown) => {
        setBusy(null);
        onFailed(describe(cause, 'Could not write the backup'));
      },
    );
  }

  function runPreview() {
    setBusy('preview');
    setPreview(null);
    backupPreview(restorePass).then(
      (result) => {
        setBusy(null);
        if (result !== null) setPreview(result);
      },
      (cause: unknown) => {
        setBusy(null);
        onFailed(describe(cause, 'Could not open that backup'));
      },
    );
  }

  function runRestore(allowReplace: boolean) {
    if (!preview) return;
    setBusy('restore');
    const replacing = preview.mode === 'replace' || preview.mode === 'fresh';
    backupRestore(preview.path, restorePass, allowReplace).then(
      (applied) => {
        setBusy(null);
        setPreview(null);
        setRestorePass('');
        onDone(
          applied.mode === 'merge'
            ? `Restored ${String(applied.created)} new and ${String(applied.merged)} updated items`
            : 'Vault restored. Unlock it with its own master password.',
        );
        // A fresh or replacing restore writes a whole new vault file and locks the
        // session, so the shell has to go back to the lock screen rather than keep
        // rendering a vault that no longer exists.
        if (replacing) onVaultReplaced();
      },
      (cause: unknown) => {
        setBusy(null);
        onFailed(describe(cause, 'The restore was not applied'));
      },
    );
  }

  return (
    <section
      className="bg-surface-panel min-w-0 flex-1 overflow-y-auto"
      aria-label="Backup and restore"
    >
      <div className="max-w-[704px] px-10 pt-8 pb-12">
        <button
          type="button"
          data-focus-ring
          className="text-chip text-accent duration-quick hover:bg-surface-hover flex h-6 items-center gap-1 rounded-full px-2 font-semibold transition-colors"
          onClick={onBack}
        >
          Settings
        </button>
        <h1 className="text-display tracking-display mt-2 font-bold">Backup and restore</h1>
        <p className="text-body text-text-muted mt-1 max-w-[62ch] leading-5 text-pretty">
          A backup is a single encrypted file under a passphrase of its own — not your master
          password. It contains every item, so treat the file and its passphrase as you would the
          vault.
        </p>

        <section className="mt-7">
          <SectionLabel>Export</SectionLabel>
          <GroupedList className="mt-2">
            <GroupedRow className="min-h-[60px] py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-body font-semibold">Backup passphrase</div>
                <div className="text-chip text-text-muted mt-0.5 text-pretty">
                  At least {MIN_PASSPHRASE} characters. It cannot be recovered, and without it the
                  file is unreadable.
                </div>
              </div>
              <Input
                aria-label="Backup passphrase"
                type="password"
                className="w-[220px] shrink-0"
                value={exportPass}
                placeholder="Passphrase for this file"
                onChange={(event) => {
                  setExportPass(event.target.value);
                }}
              />
            </GroupedRow>
            <GroupedRow className="min-h-[60px] py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-body font-semibold">Write the file</div>
                <div className="text-chip text-text-muted mt-0.5 text-pretty">
                  You choose where it goes. The app cannot read or write any other file.
                </div>
              </div>
              <Button disabled={!exportReady || busy !== null} onClick={runExport}>
                {busy === 'export' ? 'Exporting…' : 'Export backup'}
              </Button>
            </GroupedRow>
          </GroupedList>
        </section>

        <section className="mt-7">
          <SectionLabel>Restore</SectionLabel>
          <GroupedList className="mt-2">
            <GroupedRow className="min-h-[60px] py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-body font-semibold">The backup&rsquo;s passphrase</div>
                <div className="text-chip text-text-muted mt-0.5 text-pretty">
                  Whatever the file was written under. Nothing is applied until you have seen what
                  it would do.
                </div>
              </div>
              <Input
                aria-label="The backup's passphrase"
                type="password"
                className="w-[220px] shrink-0"
                value={restorePass}
                placeholder="Passphrase of the file"
                onChange={(event) => {
                  setRestorePass(event.target.value);
                }}
              />
            </GroupedRow>
            <GroupedRow className="min-h-[60px] py-2.5">
              <div className="min-w-0 flex-1">
                <div className="text-body font-semibold">Choose a file</div>
                <div className="text-chip text-text-muted mt-0.5 text-pretty">
                  Opens the file, checks its signature, and reports what a restore would change.
                </div>
              </div>
              <Button
                variant="outline"
                disabled={!restoreReady || busy !== null}
                onClick={runPreview}
              >
                {busy === 'preview' ? 'Opening…' : 'Preview a backup'}
              </Button>
            </GroupedRow>
          </GroupedList>
        </section>

        {preview === null ? null : (
          <Card className="mt-4">
            <SectionLabel className="h-auto">
              {preview.mode === 'merge'
                ? 'Merge into this vault'
                : preview.mode === 'fresh'
                  ? 'Restore into an empty device'
                  : 'Replace this vault'}
            </SectionLabel>
            <p className="text-body text-text-secondary mt-3 leading-5 text-pretty">
              {preview.mode === 'merge'
                ? `Written ${new Date(preview.createdAt).toLocaleString()}. This backup belongs to this account: ${String(preview.created)} items would be added, ${String(preview.merged)} updated, and ${String(preview.skipped)} left alone because this device already has them at the same or a newer revision.`
                : preview.mode === 'fresh'
                  ? `Written ${new Date(preview.createdAt).toLocaleString()}. There is no vault on this device, so all ${String(preview.created)} items would be created.`
                  : `Written ${new Date(preview.createdAt).toLocaleString()}. This backup belongs to a different account, so nothing in it can be merged. Restoring it replaces the vault on this device, and everything currently in it is lost.`}
            </p>

            {preview.mode === 'replace' ? (
              <p className="text-chip text-status-danger mt-3 flex items-start gap-1.5 leading-4 text-pretty">
                <Glyph name="lock" size={12} />
                This cannot be undone. Export a backup of the current vault first if you want to
                keep it.
              </p>
            ) : null}

            <div className="mt-4 flex items-center gap-2.5">
              <div className="flex-1" />
              <Button
                variant="outline"
                disabled={busy !== null}
                onClick={() => {
                  setPreview(null);
                }}
              >
                Cancel
              </Button>
              <Button
                disabled={busy !== null}
                onClick={() => {
                  runRestore(preview.mode === 'replace');
                }}
              >
                {busy === 'restore'
                  ? 'Restoring…'
                  : preview.mode === 'replace'
                    ? 'Replace the vault'
                    : 'Apply the restore'}
              </Button>
            </div>
          </Card>
        )}

        <section
          className="bg-surface-raised shadow-card mt-7 rounded-lg p-4"
          aria-labelledby="backup-facts"
        >
          <h2
            className="text-micro tracking-label text-text-muted flex h-6 items-end font-bold uppercase"
            id="backup-facts"
          >
            What a backup is
          </h2>
          <ul className="text-caption text-text-secondary mt-3 flex flex-col gap-2.5 leading-4 text-pretty">
            <li>
              <strong className="text-text-primary font-medium">One encrypted file.</strong> Every
              item, sealed under a key derived from the passphrase you choose here, with its own
              salt and the same work factor as your vault.
            </li>
            <li>
              <strong className="text-text-primary font-medium">Signed.</strong> The file carries a
              manifest signature that is checked before anything is read, so a file altered in
              transit is refused rather than partially applied.
            </li>
            <li>
              <strong className="text-text-primary font-medium">Never uploaded.</strong> The app
              writes it where you point it and does nothing else with it. There is no cloud backup
              in this version.
            </li>
          </ul>
        </section>
      </div>
    </section>
  );
}

/** Map an IPC failure onto a sentence, without naming what failed to decrypt. */
function describe(cause: unknown, fallback: string): string {
  if (!(cause instanceof IpcError)) return fallback;
  switch (cause.error.kind) {
    case 'wrongPassword':
      return 'That passphrase does not open the file.';
    case 'tamperDetected':
      // Fail closed and say so: this is not a retry-able state.
      return 'That file failed its integrity check and was not read.';
    case 'invalid':
      return 'Replacing the vault was not confirmed.';
    case 'locked':
      return 'Unlock the vault first.';
    default:
      return fallback;
  }
}
