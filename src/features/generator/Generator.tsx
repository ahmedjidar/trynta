// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Generator — components.md §10, SPEC-V1 §7.3.
 *
 * The output card, the 3-up type control, the option group and the history group, in
 * the design's order.
 *
 * **Every value is generated in Rust.** `generator_password` and friends return the
 * value and its entropy; nothing here implements a character set, a dice roll or an
 * entropy calculation. The design's own logic estimates strength with a
 * `length * log2(charset)` approximation, which is the kind of thing §7.3 has an exact
 * inclusion–exclusion implementation for — so the number on screen is the one AC12
 * cross-checks, not a re-derivation.
 *
 * **The output is a real secret**, and the history is a list of them. §7.5 has a "clear
 * generator history" control for that reason, and the blob lives in the encrypted
 * `app_cache`.
 *
 * ## One place the design cannot be implemented as drawn
 *
 * §10's history rows show each past value in monospace. `HistoryEntryDto` carries `id`,
 * `kind`, `entropyBits` and `createdAt` — and **no value**. That is deliberate: sending
 * a list of previously generated passwords over IPC on every render would put a pile of
 * live secrets in the webview for a convenience, and `generator_history_copy(id)` copies
 * in Rust without any of them crossing the boundary.
 *
 * So the rows show what was generated and when, and Copy still works. The invariant wins
 * over the appearance (handoffs/README.md), and it is recorded in handoffs/MANIFEST.md.
 */

import { useCallback, useEffect, useState } from 'react';

import { Button } from '../../components/Button';
import { CopyAction } from '../../components/Bits';
import { FieldLabel, GroupedList, GroupedRow, SectionLabel } from '../../components/GroupedList';
import { SegmentedControl } from '../../components/SegmentedControl';
import { StrengthMeter } from '../../components/StrengthMeter';
import { Switch } from '../../components/Switch';
import { Glyph } from '../../components/Glyph';
import {
  generatorHistoryClear,
  generatorHistoryCopy,
  generatorHistoryList,
  generatorPassphrase,
  generatorPassword,
  generatorPin,
} from '../../ipc';
import type { GeneratedDto, HistoryEntryDto } from '../../ipc';

/** The three types, in the design's order. */
const TYPES = [
  { id: 'password', name: 'Password' },
  { id: 'passphrase', name: 'Passphrase' },
  { id: 'pin', name: 'Numeric PIN' },
] as const;

type GeneratorType = (typeof TYPES)[number]['id'];

/** §10: the slider runs 8–64. */
const MIN_LENGTH = 8;
const MAX_LENGTH = 64;

/** Human label per generated kind, for the history rows. */
const KIND_LABELS: Record<string, string> = {
  password: 'Password',
  passphrase: 'Passphrase',
  pin: 'PIN',
};

/** Band from entropy bits, using §7.4's own thresholds rather than a new scale. */
function bandFor(bits: number): { band: number; label: string } {
  if (bits >= 90) return { band: 4, label: 'Excellent' };
  if (bits >= 65) return { band: 3, label: 'Strong' };
  if (bits >= 40) return { band: 2, label: 'Fair' };
  if (bits > 0) return { band: 1, label: 'Weak' };
  return { band: 0, label: '' };
}

export interface GeneratorProps {
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

export function Generator({ onCopied, onFailed }: GeneratorProps) {
  const [type, setType] = useState<GeneratorType>('password');
  const [length, setLength] = useState(20);
  const [lower, setLower] = useState(true);
  const [upper, setUpper] = useState(true);
  const [digits, setDigits] = useState(true);
  const [symbols, setSymbols] = useState(true);
  const [avoidAmbiguous, setAvoidAmbiguous] = useState(false);

  const [generated, setGenerated] = useState<GeneratedDto | null>(null);
  const [unavailable, setUnavailable] = useState<string | null>(null);
  const [history, setHistory] = useState<readonly HistoryEntryDto[]>([]);

  const refreshHistory = useCallback(() => {
    generatorHistoryList().then(setHistory, () => {
      setHistory([]);
    });
  }, []);

  const regenerate = useCallback(() => {
    // Nothing is set synchronously here. The effect below calls this on every recipe
    // change, and a synchronous setState inside an effect is a cascading render — so
    // both the success and the failure path clear `unavailable` from inside the promise
    // instead of a `setUnavailable(null)` up front.
    const request =
      type === 'password'
        ? generatorPassword({
            length,
            lowercase: lower,
            uppercase: upper,
            digits,
            symbols,
            avoidAmbiguous,
          })
        : type === 'passphrase'
          ? generatorPassphrase({
              // The slider is a character count; a passphrase is counted in words. Four
              // characters per word keeps one control meaningful for both.
              words: Math.max(3, Math.min(12, Math.round(length / 4))),
              separator: '-',
              capitalise: false,
              numericSuffix: false,
            })
          : generatorPin(Math.max(4, Math.min(12, length)));

    request.then(
      (next) => {
        setUnavailable(null);
        setGenerated(next);
        refreshHistory();
      },
      (error: unknown) => {
        setGenerated(null);
        // §7.3's wordlist is not bundled, so passphrase generation reports
        // `featureUnavailable` rather than falling back to a short list and claiming
        // the entropy of a complete one. Say which, rather than "failed".
        const kind =
          typeof error === 'object' && error !== null && 'error' in error
            ? (error as { error: { kind?: string } }).error.kind
            : undefined;
        setUnavailable(
          kind === 'featureUnavailable'
            ? 'Passphrases need the EFF wordlist, which is not bundled in this build.'
            : kind === 'locked' || kind === 'noVault'
              ? 'Unlock the vault to generate.'
              : 'Could not generate.',
        );
      },
    );
  }, [type, length, lower, upper, digits, symbols, avoidAmbiguous, refreshHistory]);

  // Regenerate whenever the recipe changes, which is what the design shows: the output
  // is never stale relative to the controls above it.
  useEffect(() => {
    regenerate();
  }, [regenerate]);

  const strength = bandFor(generated?.entropyBits ?? 0);

  return (
    <section
      data-scroll-pane
      className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
      aria-label="Generator"
    >
      <div className="mx-auto w-full max-w-[var(--measure-pane-wide)] px-10 pt-8 pb-12">
        <h1 className="text-display tracking-display font-bold">Generator</h1>
        <p className="text-body text-text-muted mt-1 max-w-[60ch] leading-5 text-pretty">
          {/* The design says "generated locally on this Mac". Platform-neutral here,
              since Windows is the verified platform (ADD-005) and the claim is about the
              device rather than the make of it. */}
          Every password is generated on this device. Nothing leaves it unencrypted.
        </p>

        <div className="bg-surface-raised shadow-card mt-6 rounded-lg p-5">
          <output
            className="text-secret-xl block min-h-16 font-mono font-medium tracking-tight break-all"
            data-selectable
          >
            {unavailable ?? generated?.value ?? ''}
          </output>
          {generated !== null && !generated.recorded ? (
            <p className="text-chip text-text-muted mt-2">
              Not saved to history — the vault is locked. Select the value above to copy it by hand.
            </p>
          ) : null}
          <div className="mt-4 flex items-center gap-3">
            <StrengthMeter
              score={strength.band}
              label={strength.label}
              className="w-40 flex-none"
            />
            <span className="text-caption text-text-muted tabular-nums">
              {generated === null
                ? '—'
                : `${String(Math.round(generated.entropyBits))} bits of entropy`}
            </span>
            <div className="flex-1" />
            <Button variant="outline" onClick={regenerate}>
              Regenerate
            </Button>
            <Button
              onClick={() => {
                const first = history[0];
                if (!first) {
                  onFailed('Unlock the vault to copy');
                  return;
                }
                generatorHistoryCopy(first.id).then(
                  () => {
                    onCopied('Password copied');
                  },
                  () => {
                    onFailed('Could not copy');
                  },
                );
              }}
              // Copying goes through the history entry, which cannot be written while the
              // vault is locked — generating needs no key, the encrypted history does. A
              // disabled button with the value still on screen and selectable beats one
              // that fails when pressed.
              disabled={generated === null || !generated.recorded}
            >
              Copy
            </Button>
          </div>
        </div>

        <SegmentedControl
          className="mt-5"
          segments={TYPES}
          value={type}
          onChange={setType}
          label="Type"
        />

        <GroupedList className="mt-4">
          <GroupedRow className="h-[52px]">
            <FieldLabel>Length</FieldLabel>
            <input
              className="slider min-w-0 flex-1"
              type="range"
              min={MIN_LENGTH}
              max={MAX_LENGTH}
              value={length}
              aria-label="Length"
              onChange={(event) => {
                setLength(Number(event.target.value));
              }}
            />
            <span className="text-body-lg w-7 shrink-0 text-right font-bold tabular-nums">
              {length}
            </span>
          </GroupedRow>

          {type === 'password' ? (
            <>
              <OptionRow name="Lowercase" hint="a–z" checked={lower} onChange={setLower} />
              <OptionRow name="Uppercase" hint="A–Z" checked={upper} onChange={setUpper} />
              <OptionRow name="Digits" hint="0–9" checked={digits} onChange={setDigits} />
              <OptionRow name="Symbols" hint="!@#$" checked={symbols} onChange={setSymbols} />
              <OptionRow
                name="Avoid ambiguous characters"
                hint="l 1 I O 0"
                checked={avoidAmbiguous}
                onChange={setAvoidAmbiguous}
              />
            </>
          ) : null}
        </GroupedList>

        <SectionLabel className="mt-6">Recently generated</SectionLabel>
        <GroupedList className="mt-2">
          {history.length === 0 ? (
            <GroupedRow className="h-11">
              <span className="text-body text-text-muted min-w-0 flex-1">
                Nothing generated yet.
              </span>
            </GroupedRow>
          ) : (
            history.map((entry) => (
              <GroupedRow key={entry.id} className="h-11">
                {/* The design prints the generated value in each history row.
                    `HistoryEntryDto` deliberately does not carry one — SPEC-V1 §6 gives
                    history a copy command and no reveal, so twenty old passwords are never
                    rendered into the webview. Kind and strength are enough to pick the
                    right entry. */}
                <span className="text-control text-text-secondary min-w-0 flex-1 truncate font-mono">
                  {KIND_LABELS[entry.kind] ?? 'Value'} · {Math.round(entry.entropyBits)} bits
                </span>
                <span className="text-chip text-text-muted shrink-0">
                  {new Date(entry.createdAt).toLocaleTimeString()}
                </span>
                <CopyAction
                  aria-label="Copy this generated value"
                  onClick={() => {
                    generatorHistoryCopy(entry.id).then(
                      () => {
                        onCopied('Copied');
                      },
                      () => {
                        onFailed('That entry has expired');
                      },
                    );
                  }}
                >
                  Copy
                </CopyAction>
              </GroupedRow>
            ))
          )}
        </GroupedList>

        {history.length === 0 ? null : (
          <footer className="mt-4 flex justify-end">
            <button
              type="button"
              data-focus-ring
              className="text-chip text-text-secondary duration-quick hover:bg-surface-hover hover:text-accent flex h-6 items-center gap-1.5 rounded-full px-2 font-semibold transition-colors"
              onClick={() => {
                generatorHistoryClear().then(
                  () => {
                    setHistory([]);
                    onCopied('History cleared');
                  },
                  () => {
                    onFailed('Could not clear history');
                  },
                );
              }}
            >
              <Glyph name="close" size={12} />
              Clear history
            </button>
          </footer>
        )}
      </div>
    </section>
  );
}

interface OptionRowProps {
  name: string;
  hint: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}

function OptionRow({ name, hint, checked, onChange }: OptionRowProps) {
  const toggle = () => {
    onChange(!checked);
  };
  return (
    <GroupedRow interactive className="h-[52px]" onClick={toggle}>
      <span className="text-body min-w-0 flex-1 font-medium">{name}</span>
      <span className="text-chip text-text-muted font-mono">{hint}</span>
      <Switch checked={checked} onChange={toggle} label={name} />
    </GroupedRow>
  );
}
