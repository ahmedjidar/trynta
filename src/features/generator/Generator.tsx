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
 * over the appearance (handoffs/README.md), and it is recorded for HO-002.
 */

import { useCallback, useEffect, useState } from 'react';

import {
  Button,
  Group,
  GroupRow,
  Segmented,
  StrengthMeter,
  Switch,
} from '../../components/Controls';
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
  { value: 'password', label: 'Password' },
  { value: 'passphrase', label: 'Passphrase' },
  { value: 'pin', label: 'Numeric PIN' },
] as const;

type GeneratorType = (typeof TYPES)[number]['value'];

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
    <section className="pane" aria-label="Generator">
      <div className="pane__content">
        <h1 className="pane__title">Generator</h1>
        <p className="pane__prose">
          {/* The design says "generated locally on this Mac". Platform-neutral, since
              Windows is the verified platform (ADD-005) and the claim is about the
              device, not the OS. */}
          Every password is generated on this device. Nothing leaves it unencrypted.
        </p>

        <div className="output-card">
          <output className="output-card__secret" data-selectable>
            {unavailable ?? generated?.value ?? ''}
          </output>
          <div className="output-card__footer">
            <div className="output-card__meter">
              <StrengthMeter filled={strength.band} label={strength.label} />
            </div>
            <span className="output-card__entropy">
              {generated === null
                ? '—'
                : `${String(Math.round(generated.entropyBits))} bits of entropy`}
            </span>
            <span className="detail-spacer" />
            <Button variant="outline" onClick={regenerate}>
              Regenerate
            </Button>
            <Button
              onClick={() => {
                const first = history[0];
                if (!first) {
                  onFailed('Nothing to copy');
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
              disabled={generated === null}
            >
              Copy
            </Button>
          </div>
        </div>

        <div className="generator__type">
          <Segmented options={TYPES} value={type} onChange={setType} label="Type" />
        </div>

        <Group>
          <GroupRow height="option">
            <span className="field-label field-label--strong">Length</span>
            <input
              className="slider"
              type="range"
              min={MIN_LENGTH}
              max={MAX_LENGTH}
              value={length}
              aria-label="Length"
              onChange={(event) => {
                setLength(Number(event.target.value));
              }}
            />
            <span className="slider__value">{length}</span>
          </GroupRow>

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
        </Group>

        <Group label="Recently generated">
          {history.length === 0 ? (
            <GroupRow height="history">
              <span className="field-value">Nothing generated yet.</span>
            </GroupRow>
          ) : (
            history.map((entry) => (
              <GroupRow key={entry.id} height="history">
                {/* No value. `HistoryEntryDto` does not carry one — see the module note.
                    What it is and how strong it was is enough to pick the right entry,
                    and Copy does the rest in Rust. */}
                <span className="field-value">
                  {KIND_LABELS[entry.kind] ?? 'Value'} · {Math.round(entry.entropyBits)} bits
                </span>
                <span className="history__when">
                  {new Date(entry.createdAt).toLocaleTimeString()}
                </span>
                <button
                  type="button"
                  className="copy-action"
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
                </button>
              </GroupRow>
            ))
          )}
        </Group>

        {history.length === 0 ? null : (
          <footer className="generator__footer">
            <button
              type="button"
              className="link-button"
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
              <Glyph name="close" />
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
  return (
    <GroupRow height="option">
      <span className="field-label field-label--strong field-label--wide">{name}</span>
      <span className="option-hint">{hint}</span>
      <Switch checked={checked} onChange={onChange} label={name} />
    </GroupRow>
  );
}
