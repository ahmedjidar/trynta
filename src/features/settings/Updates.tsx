// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Updater surface — SPEC-V1 §7.5, §9.
 *
 * The design has no update screen: it covers the vault, and updates are one of the
 * settings rows §7.5 asks for without drawing. So this reuses §13's grouped-row structure
 * rather than inventing a layout, and every value comes from `update_check`.
 *
 * ## Why the copy is this blunt
 *
 * An update check is one of exactly three outbound requests the whole product makes
 * (CLAUDE.md §4.7), and it is the only one that happens without the user asking. §7.5
 * requires *"a plain statement of exactly what leaves the device"*, so the endpoint's view
 * of the request — IP, version, platform — is stated on the surface that performs it, not
 * only in a settings paragraph.
 *
 * ## Five statuses, and none of them is a shrug
 *
 * `UpdateStatusDto` has five variants and only `upToDate` means everything is fine.
 * `checkFailed` in particular must not read as "you are up to date": a failed check is
 * an unknown, and §4.10's fail-closed applies to what the UI claims as much as to what
 * the code does.
 */

import { useCallback, useEffect, useState } from 'react';

import { Button } from '../../components/Button';
import { GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { IpcError, updateCheck, updateInstall } from '../../ipc';
import type { UpdateCheckDto } from '../../ipc';

/**
 * Why the check could not answer.
 *
 * `unconfigured` is not a fault: this build ships no release endpoint and no signing
 * public key (ADD-004 ⑥), so there is nothing to ask. Saying "could not be read" for that
 * describes a broken app rather than an unfinished channel.
 */
type Unavailable = 'unconfigured' | 'failed';

function classify(cause: unknown): Unavailable {
  return cause instanceof IpcError && cause.error.kind === 'featureUnavailable'
    ? 'unconfigured'
    : 'failed';
}

export interface UpdatesProps {
  /** Back to the settings list. */
  onBack: () => void;
  onFailed: (message: string) => void;
}

export function Updates({ onBack, onFailed }: UpdatesProps) {
  const [check, setCheck] = useState<UpdateCheckDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<Unavailable | null>(null);

  const run = useCallback(() => {
    setBusy(true);
    updateCheck().then(
      (result) => {
        setBusy(false);
        setFailed(null);
        setCheck(result);
      },
      (cause: unknown) => {
        setBusy(false);
        setFailed(classify(cause));
      },
    );
  }, []);

  useEffect(() => {
    // Reading the last result is not itself a network call: `update_check` enforces
    // §7.5's 24-hour cadence in Rust and returns `checkedRecently` inside the window.
    // So opening this screen cannot be a way to poll the endpoint.
    //
    // Deliberately not `run()`: that sets `busy` synchronously, and a synchronous
    // setState inside an effect is what the React compiler rule forbids. There is
    // nothing to report as busy on the first read anyway — the surface has not drawn
    // its rows yet.
    let live = true;
    updateCheck().then(
      (result) => {
        if (live) setCheck(result);
      },
      (cause: unknown) => {
        if (live) setFailed(classify(cause));
      },
    );
    return () => {
      live = false;
    };
  }, []);

  return (
    <section
      data-scroll-pane
      className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
      aria-label="Updates"
    >
      <div className="mx-auto w-full max-w-[var(--measure-pane-wide)] px-10 pt-8 pb-12">
        <button
          type="button"
          data-focus-ring
          className="text-chip text-accent duration-quick hover:bg-surface-hover flex h-6 items-center gap-1 rounded-full px-2 font-semibold transition-colors"
          onClick={onBack}
        >
          Settings
        </button>
        <h1 className="text-display tracking-display mt-2 font-bold">Updates</h1>

        {failed !== null || check === null ? (
          <p className="text-body text-text-muted mt-2 max-w-[62ch] leading-5 text-pretty">
            {failed === 'unconfigured'
              ? 'This build has no update channel: no release endpoint and no signing key are configured, so there is nothing to check against. Updates arrive with the first signed release.'
              : failed === 'failed'
                ? 'The update state could not be read. Nothing was downloaded and nothing was installed.'
                : ''}
          </p>
        ) : (
          <>
            <section className="mt-7">
              <SectionLabel>This build</SectionLabel>
              <GroupedList className="mt-2">
                <GroupedRow className="min-h-[60px] py-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="text-body font-semibold">Version</div>
                    <div className="text-chip text-text-muted mt-0.5 text-pretty">
                      {describe(check)}
                    </div>
                  </div>
                  <span className="text-control shrink-0 tabular-nums">{check.currentVersion}</span>
                </GroupedRow>

                <GroupedRow className="min-h-[60px] py-2.5">
                  <div className="min-w-0 flex-1">
                    <div className="text-body font-semibold">Automatic checks</div>
                    <div className="text-chip text-text-muted mt-0.5 text-pretty">
                      {check.checksEnabled
                        ? 'On: at most once every 24 hours, on launch.'
                        : 'Off. This screen still checks when you ask it to.'}
                    </div>
                  </div>
                  <span className="text-control text-text-muted shrink-0">
                    {check.checksEnabled ? 'On' : 'Off'}
                  </span>
                </GroupedRow>
              </GroupedList>
            </section>

            {check.available === null ? null : (
              <section className="mt-7">
                <SectionLabel>Available</SectionLabel>
                <GroupedList className="mt-2">
                  <GroupedRow className="min-h-[60px] py-2.5">
                    <div className="min-w-0 flex-1">
                      <div className="text-body font-semibold">{check.available.version}</div>
                      <div className="text-chip text-text-muted mt-0.5 text-pretty">
                        {check.available.notes ??
                          'No release notes were included in the signed manifest.'}
                      </div>
                    </div>
                    <Button
                      variant="primary"
                      disabled={busy}
                      onClick={() => {
                        setBusy(true);
                        updateInstall().then(
                          () => {
                            setBusy(false);
                          },
                          () => {
                            setBusy(false);
                            // §9: a failed signature check is not a retry. The message
                            // never suggests one.
                            onFailed('The update was not installed');
                          },
                        );
                      }}
                    >
                      {busy ? 'Working…' : 'Install and restart'}
                    </Button>
                  </GroupedRow>
                </GroupedList>
              </section>
            )}

            <div className="mt-6 flex h-8 items-center">
              <div className="flex-1" />
              <Button variant="outline" onClick={run} disabled={busy}>
                {busy ? 'Checking…' : 'Check now'}
              </Button>
            </div>
          </>
        )}

        <section
          className="bg-surface-raised shadow-card mt-7 rounded-lg p-4"
          aria-labelledby="update-privacy"
        >
          <h2
            className="text-micro tracking-label text-text-muted flex h-6 items-end font-bold uppercase"
            id="update-privacy"
          >
            What an update check sends
          </h2>
          <ul className="text-caption text-text-secondary mt-3 flex flex-col gap-2.5 leading-4 text-pretty">
            <li>
              A request for a <strong>signed manifest</strong>. The endpoint learns your IP address,
              the version you are running and your platform.
            </li>
            <li>
              <strong>Nothing about your vault.</strong> Not how many items it holds, not their
              names, not whether it has ever been unlocked.
            </li>
            <li>
              The manifest&rsquo;s signature is verified before anything is installed. A manifest
              that fails verification is discarded, and no partial update is applied.
            </li>
          </ul>
        </section>
      </div>
    </section>
  );
}

/** One sentence per status. `checkFailed` must not read like `upToDate`. */
function describe(check: UpdateCheckDto): string {
  switch (check.status) {
    case 'upToDate':
      return 'This is the newest version the manifest offers.';
    case 'available':
      return 'A newer version is available.';
    case 'checkedRecently':
      return 'Checked within the last 24 hours. The result below is that check, not a new one.';
    case 'checkFailed':
      // Not "up to date". An unknown is an unknown.
      return 'The last check did not complete, so whether a newer version exists is unknown.';
    case 'disabled':
      return 'Automatic checks are off, so nothing has been checked.';
  }
}
