/**
 * Settings — components.md §13, SPEC-V1 §7.5.
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
 * Both conflicts are HO-002 item 4 in `handoffs/MANIFEST.md`.
 *
 * ## The statement at the bottom is not filler
 *
 * §7.5 asks for *"a plain statement of exactly what leaves the device"*. There are exactly
 * three permitted outbound requests (CLAUDE.md §4.7) and one of them is "nothing else", so
 * the list is short and checkable — which is the point of writing it where a user can read
 * it rather than only in a spec.
 */

import { useCallback, useState } from 'react';

import { Group, GroupRow, Switch } from '../../components/Controls';
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
    <section className="pane" aria-label="Settings">
      <div className="pane__content">
        <h1 className="pane__title">Settings</h1>

        <Group label="Security">
          <GroupRow height="setting">
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
              onChange={(value) => {
                patch({ biometricEnabled: value });
              }}
            />
          </GroupRow>

          <GroupRow height="setting">
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
              onChange={(value) => {
                patch({ clearClipboard: value });
              }}
            />
          </GroupRow>

          {settings.clearClipboard ? (
            <GroupRow height="setting">
              <RowText name="Clear after" description="How long a copied secret stays available." />
              <select
                className="setting-select"
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
            </GroupRow>
          ) : null}

          <GroupRow height="setting">
            <RowText
              name="Require the master password to reveal"
              description="Off by default: the rolling 20-per-minute limit already asks for re-auth, and typing a master password constantly is its own risk."
            />
            <Switch
              checked={settings.requireMasterOnReveal}
              disabled={saving}
              label="Require the master password to reveal"
              onChange={(value) => {
                patch({ requireMasterOnReveal: value });
              }}
            />
          </GroupRow>

          <GroupRow height="setting">
            <RowText
              name="Watch for breaches"
              description="Checks 5-character hash prefixes at most once a day. Passwords never leave this device."
            />
            <Switch
              checked={settings.watchForBreaches}
              disabled={saving}
              label="Watch for breaches"
              onChange={(value) => {
                patch({ watchForBreaches: value });
              }}
            />
          </GroupRow>

          <GroupRow height="setting">
            <RowText
              name="Hide from screen capture"
              description="Off by default: it matches the stated threat model, and turning it on breaks your own screenshots and screen sharing for support."
            />
            <Switch
              checked={settings.contentProtection}
              disabled={saving}
              label="Hide from screen capture"
              onChange={(value) => {
                patch({ contentProtection: value });
              }}
            />
          </GroupRow>
        </Group>

        <Group label="Appearance">
          <GroupRow height="setting">
            <RowText name="Theme" description="Dark, light, or whatever the system is set to." />
            <select
              className="setting-select"
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
          </GroupRow>

          <GroupRow height="setting">
            <RowText
              name="Imported themes"
              description={
                importedThemes.length === 0
                  ? 'A theme is a set of colour values. Imported themes are validated in Rust before anything is applied.'
                  : `${String(importedThemes.length)} imported.`
              }
            />
            <span className="setting-value">{settings.importedThemeCount}</span>
          </GroupRow>
        </Group>

        <Group label="Autofill and import">
          <GroupRow height="setting">
            <RowText
              name="Autofill"
              description="Not available yet. Autofill and import arrive in a later version, and matching is on the registrable domain — never a substring."
            />
            {/* §7.5: "Never a toggle that does nothing." A stated fact, not a switch. */}
            <span className="setting-value setting-value--muted">Not in this version</span>
          </GroupRow>
        </Group>

        <Group label="Privacy and data">
          <GroupRow height="setting" onClick={onBackup} label="Backup and restore">
            <RowText
              name="Backup and restore"
              description="Export an encrypted backup under its own passphrase, or restore from one."
            />
            <span className="setting-chevron">
              <Glyph name="next" />
            </span>
          </GroupRow>

          <GroupRow height="setting" onClick={onUpdates} label="Updates">
            <RowText
              name="Updates"
              description={
                settings.updateChecksEnabled
                  ? 'Checked at most once a day. The endpoint learns your IP, version and platform, and nothing else.'
                  : 'Automatic checks are off. You can still check manually.'
              }
            />
            <span className="setting-chevron">
              <Glyph name="next" />
            </span>
          </GroupRow>

          <GroupRow height="setting">
            <RowText
              name="Check for updates automatically"
              description="At most once every 24 hours, on launch."
            />
            <Switch
              checked={settings.updateChecksEnabled}
              disabled={saving}
              label="Check for updates automatically"
              onChange={(value) => {
                patch({ updateChecksEnabled: value });
              }}
            />
          </GroupRow>
        </Group>

        {/* §7.5: "a plain statement of exactly what leaves the device". */}
        <section className="card card--notes" aria-labelledby="leaves-device">
          <h2 className="card__label" id="leaves-device">
            What leaves this device
          </h2>
          <ul className="leaves-list">
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
    <span className="setting-text">
      <span className="setting-name">{name}</span>
      <span className="setting-description">{description}</span>
    </span>
  );
}
