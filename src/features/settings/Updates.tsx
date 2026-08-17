/**
 * Updater surface — SPEC-V1 §7.5, §9.
 *
 * The design has no update screen: HO-001 covers the vault, and updates are one of the
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

import { Button, Group, GroupRow } from '../../components/Controls';
import { updateCheck, updateInstall } from '../../ipc';
import type { UpdateCheckDto } from '../../ipc';

export interface UpdatesProps {
  /** Back to the settings list. */
  onBack: () => void;
  onFailed: (message: string) => void;
}

export function Updates({ onBack, onFailed }: UpdatesProps) {
  const [check, setCheck] = useState<UpdateCheckDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  const run = useCallback(() => {
    setBusy(true);
    updateCheck().then(
      (result) => {
        setBusy(false);
        setFailed(false);
        setCheck(result);
      },
      () => {
        setBusy(false);
        setFailed(true);
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
      () => {
        if (live) setFailed(true);
      },
    );
    return () => {
      live = false;
    };
  }, []);

  return (
    <section className="pane" aria-label="Updates">
      <div className="pane__content">
        <button type="button" className="pane__back" onClick={onBack}>
          Settings
        </button>
        <h1 className="pane__title">Updates</h1>

        {failed || check === null ? (
          <p className="pane__prose">
            {failed
              ? 'The update state could not be read. Nothing was downloaded and nothing was installed.'
              : ''}
          </p>
        ) : (
          <>
            <Group label="This build">
              <GroupRow height="setting">
                <span className="setting-text">
                  <span className="setting-name">Version</span>
                  <span className="setting-description">{describe(check)}</span>
                </span>
                <span className="setting-value">{check.currentVersion}</span>
              </GroupRow>

              <GroupRow height="setting">
                <span className="setting-text">
                  <span className="setting-name">Automatic checks</span>
                  <span className="setting-description">
                    {check.checksEnabled
                      ? 'On: at most once every 24 hours, on launch.'
                      : 'Off. This screen still checks when you ask it to.'}
                  </span>
                </span>
                <span className="setting-value setting-value--muted">
                  {check.checksEnabled ? 'On' : 'Off'}
                </span>
              </GroupRow>
            </Group>

            {check.available === null ? null : (
              <Group label="Available">
                <GroupRow height="setting">
                  <span className="setting-text">
                    <span className="setting-name">{check.available.version}</span>
                    <span className="setting-description">
                      {check.available.notes ??
                        'No release notes were included in the signed manifest.'}
                    </span>
                  </span>
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
                </GroupRow>
              </Group>
            )}

            <div className="security__section-head">
              <span className="detail-spacer" />
              <Button variant="outline" onClick={run} disabled={busy}>
                {busy ? 'Checking…' : 'Check now'}
              </Button>
            </div>
          </>
        )}

        <section className="card card--notes" aria-labelledby="update-privacy">
          <h2 className="card__label" id="update-privacy">
            What an update check sends
          </h2>
          <ul className="leaves-list">
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
