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

import { GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { Switch } from '../../components/Switch';
import { Glyph } from '../../components/Glyph';
import { useThemeStore } from '../../theme/store';
import type { ThemeMode } from '../../theme/mode';
import { settingsSet } from '../../ipc';
import type { SettingsDto, SettingsPatch } from '../../ipc';

/** Clipboard intervals offered, in seconds. §7.5's default is 30. */
const CLIPBOARD_CHOICES = [5, 15, 30, 60, 120, 300] as const;

/** Appearance modes, in the order the picker cycles them. */
const MODES: readonly { value: ThemeMode; label: string }[] = [
  { value: 'system', label: 'Match the system' },
  { value: 'dark', label: 'Dark' },
  { value: 'light', label: 'Light' },
];

export interface SettingsProps {
  /** Current settings, from `settings_get`. */
  settings: SettingsDto;
  /** Called after a successful write, with what was actually stored. */
  onSaved: (next: SettingsDto) => void;
  onFailed: (message: string) => void;
  /** Open the backup/restore surface. */
  onBackup: () => void;
  /** Open the updater surface. */
  onUpdates: () => void;
}

export function Settings({ settings, onSaved, onFailed, onBackup, onUpdates }: SettingsProps) {
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);
  const importedThemes = useThemeStore((s) => s.imported);
  const [saving, setSaving] = useState(false);

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
    <section className="bg-surface-panel min-w-0 flex-1 overflow-y-auto" aria-label="Settings">
      <div className="max-w-[704px] px-10 pt-8 pb-12">
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
              <Switch
                checked={settings.biometricEnabled}
                disabled={!settings.biometricAvailable || saving}
                label="Unlock with biometrics"
                onChange={() => {
                  patch({ biometricEnabled: !settings.biometricEnabled });
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
              <GroupedRow className="min-h-[60px] py-2.5">
                <RowText
                  name="Clear after"
                  description="How long a copied secret stays available."
                />
                <select
                  className="border-strong bg-surface-panel text-control text-text-primary h-6 max-w-[180px] shrink-0 appearance-none rounded-sm border px-2"
                  aria-label="Clear clipboard after"
                  value={settings.clipboardSeconds}
                  disabled={saving}
                  onChange={(event) => {
                    patch({ clipboardSeconds: Number(event.target.value) });
                  }}
                >
                  {CLIPBOARD_CHOICES.map((seconds) => (
                    <option key={seconds} value={seconds}>
                      {seconds < 60
                        ? `${String(seconds)} seconds`
                        : `${String(seconds / 60)} minutes`}
                    </option>
                  ))}
                </select>
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
              <RowText name="Theme" description="Dark, light, or whatever the system is set to." />
              <select
                className="border-strong bg-surface-panel text-control text-text-primary h-6 max-w-[180px] shrink-0 appearance-none rounded-sm border px-2"
                aria-label="Theme"
                value={mode}
                onChange={(event) => {
                  void setMode(event.target.value as ThemeMode);
                }}
              >
                {MODES.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </GroupedRow>

            <GroupedRow className="min-h-[60px] py-2.5">
              <RowText
                name="Imported themes"
                description={
                  importedThemes.length === 0
                    ? 'A theme is a set of colour values. Imported themes are validated in Rust before anything is applied.'
                    : `${String(importedThemes.length)} imported.`
                }
              />
              <span className="text-control shrink-0 tabular-nums">
                {settings.importedThemeCount}
              </span>
            </GroupedRow>
          </GroupedList>
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
