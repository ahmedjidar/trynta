// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Settings — SPEC-V1 §7.5.
 *
 * ## Which document owns what
 *
 * `handoffs/README.md`: *"The handoff wins on appearance. `/specs` wins on behaviour."*
 * So the **row structure, spacing, type and states** are §13's, and **which rows exist**
 * is §7.5's list. Those two disagree, and the disagreements are not cosmetic:
 *
 * - The design has "Share anonymous diagnostics — Crash reports only." CLAUDE.md §1 bans
 *   telemetry pre-1.0 and §4.7 bans a crash reporter outright. **Not built**, and not
 *   built as a disabled row either — `SettingsPatch` has no field for it, so nothing can
 *   wire it up later by accident.
 * - The design has "Autofill in Safari and Chrome" and "Browser extension" as live
 *   switches. Autofill is V3, and §7.5 requires an honest "not available yet" state
 *   rather than *"a toggle that does nothing"*. Rendered as a stated fact.
 * - The design has a "Teams & sharing" group. Sharing is V2.
 * - §7.5 has rows the design does not draw: require-master-on-reveal, hide-from-screen-
 *   capture, theme selection and import, clear activity, clear password history, backup
 *   export and restore, the update-check toggle, and *"a plain statement of exactly what
 *   leaves the device"*.
 *
 * Both conflicts are recorded in `handoffs/MANIFEST.md`.
 *
 * ## The statement at the bottom is not filler
 *
 * §7.5 asks for *"a plain statement of exactly what leaves the device"*. There are exactly
 * three permitted outbound requests (CLAUDE.md §4.7) and one of them is "nothing else", so
 * the list is short and checkable — which is the point of writing it where a user can read
 * it rather than only in a spec.
 */

import { useCallback, useState } from 'react';

import { Chip } from '../../components/Bits';
import { GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { SegmentedControl } from '../../components/SegmentedControl';
import { Switch } from '../../components/Switch';
import { Glyph } from '../../components/Glyph';
import { useThemeStore } from '../../theme/store';
import type { ThemeMode } from '../../theme/mode';
import { Button } from '../../components/Button';
import {
  biometricDisable,
  biometricEnable,
  IpcError,
  settingsGet,
  settingsSet,
  themeImportFile,
} from '../../ipc';
import { ReauthPrompt } from '../account/ReauthPrompt';
import { ImportedThemes } from './ImportedThemes';
import { ThemeFormat } from './ThemeFormat';
import type { SettingsDto, SettingsPatch } from '../../ipc';

/** Clipboard intervals offered, in seconds. §7.5's default is 30. */
const CLIPBOARD_CHOICES = [5, 15, 30, 60, 120, 300] as const;

/**
 * Appearance modes.
 *
 * A segmented control rather than a menu: three mutually exclusive options with short
 * labels is exactly what the design's segmented control is for, and a native `<select>`
 * opens a system popup that no stylesheet in this app can reach — the one control that
 * would still look like Windows inside a themed window.
 */
const MODES: readonly { id: ThemeMode; name: string }[] = [
  { id: 'system', name: 'System' },
  { id: 'dark', name: 'Dark' },
  { id: 'light', name: 'Light' },
];

/**
 * What to tell the user for each way a theme file can be refused.
 *
 * The validator has always known which token was at fault; the IPC boundary used to
 * flatten every case to "the input is not valid", for a format that had no published
 * shape. Naming the token is the difference between a file someone can fix and a file
 * they delete.
 *
 * Naming the token alone turned out not to be enough. "`--font-sans` has a value
 * Trynta will not apply" tells you where to look and nothing about what to change,
 * and the answer was a single double quote. So each message carries all three parts:
 * which token, what was found, and what is allowed instead.
 */
function themeRejection(error: unknown): string {
  if (!(error instanceof IpcError) || error.error.kind !== 'themeRejected') {
    return 'That theme could not be read.';
  }
  const { reason, token, found } = error.error;
  const named = token ?? 'a token';
  // Every message below says the same three things, because a rejection that leaves
  // any of them out sends the user back to guessing: which token, what was found in
  // it, and what is allowed instead.
  const at = found === null ? '' : ` (found ${found})`;
  switch (reason) {
    case 'malformed':
      return 'That file is not a Trynta theme. It needs id, name, mode and tokens.';
    case 'tooLarge':
      return 'That file is too large to be a theme.';
    case 'badIdentity':
      return 'That theme needs a short id and a name.';
    case 'tooManyTokens':
      return 'That theme defines more tokens than Trynta will apply.';
    case 'notACustomProperty':
      return `${named} is not a custom property. Keys must start with -- and use lowercase letters, digits and dashes.`;
    case 'forbiddenFunction':
      return `${named} uses a function that could fetch${at}. A theme is colours, sizes and easings only — nothing that reaches the network.`;
    case 'unknownFunction':
      return `${named} calls a function Trynta will not run${at}. Allowed: colour functions, var(), calc(), min(), max(), clamp(), cubic-bezier() and the filter functions.`;
    case 'forbiddenCharacter':
      return `${named} contains ${found === null ? 'a character' : `"${found}"`}, which a value may not. Allowed: letters, digits, spaces, # % . , - + * / ( ) _ and quotes.`;
    case 'commentSequence':
      return `${named} contains ${found ?? 'a comment'}. A theme value may not carry a CSS comment.`;
    case 'unbalancedQuotes':
      return `${named} has an unclosed quote. Quotes are for font family names and must come in pairs.`;
    case 'valueLength':
      return `${named} is empty or longer than 256 characters.`;
    default:
      // Unreachable while the DTO and this switch agree; a new variant lands here
      // rather than compiling to `undefined`.
      return 'That theme could not be applied.';
  }
}

export interface SettingsProps {
  /** Current settings, from `settings_get`. */
  settings: SettingsDto;
  /** Called after a successful write, with what was actually stored. */
  onSaved: (next: SettingsDto) => void;
  onFailed: (message: string) => void;
  /** Report a success to the toast. */
  onCopied: (what: string) => void;
  /** Open vault management. */
  onVaults: () => void;
  /** Open the backup/restore surface. */
  onBackup: () => void;
  /** Open the updater surface. */
  onUpdates: () => void;
}

export function Settings({
  settings,
  onSaved,
  onFailed,
  onCopied,
  onVaults,
  onBackup,
  onUpdates,
}: SettingsProps) {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const importedThemes = useThemeStore((s) => s.imported);
  const refreshThemes = useThemeStore((s) => s.refresh);
  const [saving, setSaving] = useState(false);
  const [importing, setImporting] = useState(false);
  const [enrolling, setEnrolling] = useState(false);

  const patch = useCallback(
    (next: SettingsPatch) => {
      setSaving(true);
      settingsSet(next).then(
        (stored) => {
          setSaving(false);
          // Render what was stored, not what was asked for. They differ whenever a value
          // is clamped, and showing the request would display a number that is not saved.
          onSaved(stored);
        },
        () => {
          setSaving(false);
          onFailed('Could not save that setting');
        },
      );
    },
    [onSaved, onFailed],
  );

  return (
    <>
      {enrolling ? (
        <ReauthPrompt
          reason="enrol"
          verify={biometricEnable}
          onConfirmed={() => {
            setEnrolling(false);
            settingsGet().then(onSaved, () => {
              /* enrolment already succeeded; the count is cosmetic */
            });
          }}
          onCancel={() => {
            setEnrolling(false);
          }}
        />
      ) : null}
      <section
        data-scroll-pane
        className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
        aria-label="Settings"
      >
        <div className="mx-auto w-full max-w-[var(--measure-pane-wide)] px-10 pt-8 pb-12">
          <h1 className="text-display tracking-display font-bold">Settings</h1>

          <section className="mt-7">
            <SectionLabel>Security</SectionLabel>
            <GroupedList className="mt-2">
              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Unlock with biometrics"
                  description={
                    settings.biometricAvailable
                      ? 'Your master password is still required after a restart.'
                      : 'No biometric sensor is enrolled on this device.'
                  }
                />
                {/* Turning this on used to write a boolean and nothing else, so the
                  lock screen offered an unlock that had never been enrolled. It now
                  asks for the master password once — there is no moment when the app
                  is holding it, and Rust needs it to wrap behind Hello. */}
                <Switch
                  checked={settings.biometricEnabled}
                  disabled={!settings.biometricAvailable || saving}
                  label="Unlock with biometrics"
                  onChange={() => {
                    if (settings.biometricEnabled) {
                      setSaving(true);
                      biometricDisable().then(
                        () => {
                          setSaving(false);
                          settingsGet().then(onSaved, () => {
                            /* the switch reads from settings; a failed re-read is cosmetic */
                          });
                        },
                        () => {
                          setSaving(false);
                          onFailed('That could not be turned off.');
                        },
                      );
                      return;
                    }
                    setEnrolling(true);
                  }}
                />
              </GroupedRow>

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Clear clipboard after copying"
                  description={
                    settings.clearClipboard
                      ? `Copied secrets are wiped after ${String(settings.clipboardSeconds)} seconds.`
                      : 'Copied secrets stay on the clipboard until something replaces them.'
                  }
                />
                <Switch
                  checked={settings.clearClipboard}
                  disabled={saving}
                  label="Clear clipboard after copying"
                  onChange={() => {
                    patch({ clearClipboard: !settings.clearClipboard });
                  }}
                />
              </GroupedRow>

              {settings.clearClipboard ? (
                <GroupedRow className="min-h-[60px] gap-3 py-2.5">
                  <RowText
                    name="Clear after"
                    description="How long a copied secret stays available."
                  />
                  <div
                    className="flex shrink-0 flex-wrap justify-end gap-1.5"
                    role="group"
                    aria-label="Clear clipboard after"
                  >
                    {CLIPBOARD_CHOICES.map((seconds) => (
                      <Chip
                        key={seconds}
                        selected={settings.clipboardSeconds === seconds}
                        disabled={saving}
                        onClick={() => {
                          patch({ clipboardSeconds: seconds });
                        }}
                      >
                        {seconds < 60 ? `${String(seconds)}s` : `${String(seconds / 60)}m`}
                      </Chip>
                    ))}
                  </div>
                </GroupedRow>
              ) : null}

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Require the master password to reveal"
                  description="Off by default: the rolling 20-per-minute limit already asks for re-auth, and typing a master password constantly is its own risk."
                />
                <Switch
                  checked={settings.requireMasterOnReveal}
                  disabled={saving}
                  label="Require the master password to reveal"
                  onChange={() => {
                    patch({ requireMasterOnReveal: !settings.requireMasterOnReveal });
                  }}
                />
              </GroupedRow>

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Watch for breaches"
                  description="Checks 5-character hash prefixes at most once a day. Passwords never leave this device."
                />
                <Switch
                  checked={settings.watchForBreaches}
                  disabled={saving}
                  label="Watch for breaches"
                  onChange={() => {
                    patch({ watchForBreaches: !settings.watchForBreaches });
                  }}
                />
              </GroupedRow>

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Hide from screen capture"
                  description="Off by default: it matches the stated threat model, and turning it on breaks your own screenshots and screen sharing for support."
                />
                <Switch
                  checked={settings.contentProtection}
                  disabled={saving}
                  label="Hide from screen capture"
                  onChange={() => {
                    patch({ contentProtection: !settings.contentProtection });
                  }}
                />
              </GroupedRow>
            </GroupedList>
          </section>

          <section className="mt-7">
            <SectionLabel>Appearance</SectionLabel>
            <GroupedList className="mt-2">
              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Theme"
                  description="Dark, light, or whatever the system is set to."
                />
                <SegmentedControl
                  className="w-[260px]"
                  segments={MODES}
                  value={mode}
                  label="Theme"
                  onChange={(next) => {
                    void setMode(next);
                  }}
                />
              </GroupedRow>

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Imported themes"
                  description={
                    importedThemes.length === 0
                      ? 'A theme is a set of colour values in a JSON file. It is validated in Rust before anything is applied — a theme cannot fetch, and cannot contain url().'
                      : `${String(importedThemes.length)} imported. Pick one from the list below.`
                  }
                />
                <span className="text-control shrink-0 tabular-nums">
                  {settings.importedThemeCount}
                </span>
                {/* The picker, the read and the validation are all in Rust. The webview
                  holds no filesystem permission and this is not the place to give it
                  one — the same reasoning as the custom-icon upload. */}
                <Button
                  variant="outline"
                  disabled={importing}
                  onClick={() => {
                    setImporting(true);
                    themeImportFile().then(
                      (theme) => {
                        setImporting(false);
                        // `null` is a cancelled dialog, which is not an event.
                        if (theme === null) return;
                        void refreshThemes();
                        settingsGet().then(onSaved, () => {
                          /* the count is cosmetic; the theme is already stored */
                        });
                      },
                      (error: unknown) => {
                        setImporting(false);
                        onFailed(themeRejection(error));
                      },
                    );
                  }}
                >
                  {importing ? 'Reading…' : 'Import a theme'}
                </Button>
              </GroupedRow>
            </GroupedList>
            <ImportedThemes onFailed={onFailed} onDone={onCopied} />
            <ThemeFormat onFailed={onFailed} onDone={onCopied} />
          </section>

          <section className="mt-7">
            <SectionLabel>Autofill and import</SectionLabel>
            <GroupedList className="mt-2">
              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Autofill"
                  description="Not available yet. Autofill and import arrive in a later version, and matching is on the registrable domain — never a substring."
                />
                {/* §7.5: "Never a toggle that does nothing." A stated fact, not a switch. */}
                <span className="text-control text-text-muted shrink-0">Not in this version</span>
              </GroupedRow>
            </GroupedList>
          </section>

          <section className="mt-7">
            <SectionLabel>Vaults</SectionLabel>
            <GroupedList className="mt-2">
              <GroupedRow
                interactive
                className="min-h-[60px] py-2.5"
                onClick={onVaults}
                role="button"
                tabIndex={0}
                data-focus-ring
                aria-label="Vaults"
              >
                <RowText
                  name="Manage vaults"
                  description="Create, rename, recolour or remove a vault. There is no limit."
                />
                <Glyph name="next" size={16} />
              </GroupedRow>
            </GroupedList>

            <SectionLabel>Privacy and data</SectionLabel>
            <GroupedList className="mt-2">
              <GroupedRow
                interactive
                className="min-h-[60px] py-2.5"
                onClick={onBackup}
                role="button"
                tabIndex={0}
                data-focus-ring
                aria-label="Backup and restore"
              >
                <RowText
                  name="Backup and restore"
                  description="Export an encrypted backup under its own passphrase, or restore from one."
                />
                <span className="text-control text-text-secondary flex shrink-0 items-center gap-0.5 font-medium">
                  <Glyph name="next" size={14} />
                </span>
              </GroupedRow>

              <GroupedRow
                interactive
                className="min-h-[60px] py-2.5"
                onClick={onUpdates}
                role="button"
                tabIndex={0}
                data-focus-ring
                aria-label="Updates"
              >
                <RowText
                  name="Updates"
                  description={
                    settings.updateChecksEnabled
                      ? 'Checked at most once a day. The endpoint learns your IP, version and platform, and nothing else.'
                      : 'Automatic checks are off. You can still check manually.'
                  }
                />
                <span className="text-control text-text-secondary flex shrink-0 items-center gap-0.5 font-medium">
                  <Glyph name="next" size={14} />
                </span>
              </GroupedRow>

              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Check for updates automatically"
                  description="At most once every 24 hours, on launch."
                />
                <Switch
                  checked={settings.updateChecksEnabled}
                  disabled={saving}
                  label="Check for updates automatically"
                  onChange={() => {
                    patch({ updateChecksEnabled: !settings.updateChecksEnabled });
                  }}
                />
              </GroupedRow>
            </GroupedList>
          </section>

          {/* §7.5: "a plain statement of exactly what leaves the device". */}
          <section
            className="bg-surface-raised shadow-card mt-7 rounded-lg p-4"
            aria-labelledby="leaves-device"
          >
            <h2
              className="text-micro tracking-label text-text-muted flex h-6 items-end font-bold uppercase"
              id="leaves-device"
            >
              What leaves this device
            </h2>
            <ul className="text-caption text-text-secondary mt-3 flex flex-col gap-2.5 leading-4 text-pretty">
              <li>
                <strong>Breach checks.</strong> The first 5 characters of a password&rsquo;s SHA-1
                hash, with padding requested so the response length reveals nothing. Never the
                password, never the rest of the hash, never which item it belongs to.
              </li>
              <li>
                <strong>Update checks.</strong> A request for a signed manifest. The endpoint learns
                your IP address, the version you are running and your platform.
              </li>
              <li>
                <strong>Nothing else.</strong> No analytics, no crash reports, no icon or favicon
                fetches, and the app never contacts a site you have an account with — not even to
                check whether it has a change-password page.
              </li>
            </ul>
          </section>
        </div>
      </section>
    </>
  );
}

interface RowTextProps {
  name: string;
  description: string;
}

function RowText({ name, description }: RowTextProps) {
  return (
    <div className="min-w-0 flex-1">
      <div className="text-body font-semibold">{name}</div>
      <div className="text-chip text-text-muted mt-0.5 text-pretty">{description}</div>
    </div>
  );
}
