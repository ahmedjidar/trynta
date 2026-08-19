/**
 * The one-time-code editor: paste a QR link or type the setup key.
 *
 * Services hand out two things and call both "the code" — the
 * `otpauth://totp/...` URI hidden behind the QR image, and a bare base32 string
 * for people who cannot scan one. Both go in this single box, and which one it
 * is is **detected rather than asked**: a user copying a value out of a support
 * page does not know or care which format their bank chose, and a form that made
 * them pick would be asking a question only the app can answer.
 *
 * ## Nothing is parsed here
 *
 * The text goes straight to Rust ({@link totpParse}). That is not about secrecy —
 * the user just pasted it, so the webview has already seen it, exactly as it sees
 * a typed password. It is because the parameters that reach `secret_ct` have to
 * be the ones the URI carried. A TypeScript parser that quietly dropped
 * `algorithm=SHA256` would store SHA-1 and generate codes that never work, which
 * ADD-004 §④ records having already shipped once.
 *
 * ## Rejections say which rule failed
 *
 * `AppError` carries a {@link TotpRejectionDto}, never the input, so this
 * component can be specific without a shared secret ever entering an error
 * string. "That is a counter-based code" and "check for a typo — base32 is A–Z
 * and 2–7" send the user somewhere useful; "the input is not valid" does not.
 */

import { useRef, useState } from 'react';

import { CopyAction, Input } from '../../components/Bits';
import { FieldLabel, GroupedRow } from '../../components/GroupedList';
import { IpcError, totpParse } from '../../ipc';
import type { AppError, TotpConfigInput, TotpRejectionDto } from '../../ipc';

/** What to tell the user for each way a setup key can be refused. */
const REJECTION: Record<TotpRejectionDto, string> = {
  notBase32: 'Setup keys use A–Z and 2–7 only. Check for a typo, or paste the otpauth:// link.',
  emptySecret: 'That setup key is empty.',
  truncatedSecret: 'That setup key is incomplete — it is missing characters from the end.',
  missingSecret: 'That link has no secret in it.',
  notOtpauthUri: 'That is not an otpauth:// setup link. Paste the link, or just the setup key.',
  counterBased: 'That is a counter-based (HOTP) code. Keyring supports time-based codes.',
  unsupportedDigits: 'A one-time code is 6 or 8 digits; that link asks for something else.',
  unsupportedPeriod: 'That link asks for a time step of zero seconds, which cannot work.',
};

/** The rejection reason, if this is one; `null` for any other failure. */
function rejectionOf(error: unknown): TotpRejectionDto | null {
  if (!(error instanceof IpcError)) return null;
  const app: AppError = error.error;
  return app.kind === 'totpRejected' ? app.reason : null;
}

/**
 * How a configuration differs from the RFC defaults, or `null` when it does not.
 *
 * Shown because the difference is the whole reason this went through Rust: a user
 * who pasted an 8-digit SHA-256 link should be able to see that it was understood
 * as one, rather than discovering at the login prompt that it was not.
 */
function nonDefaults(config: TotpConfigInput): string | null {
  const parts: string[] = [];
  if (config.algorithm !== 'sha1')
    parts.push(config.algorithm.toUpperCase().replace('SHA', 'SHA-'));
  if (config.digits !== 6) parts.push(`${String(config.digits)} digits`);
  if (config.periodSeconds !== 30) parts.push(`${String(config.periodSeconds)}s step`);
  return parts.length > 0 ? parts.join(' · ') : null;
}

export interface TotpFieldProps {
  /** The configuration held by the item being edited, or `null` for none. */
  value: TotpConfigInput | null;
  /** Called with a parsed configuration, or `null` when the field is cleared. */
  onChange: (next: TotpConfigInput | null) => void;
  /** Row height, so this matches whichever list it is dropped into. */
  className?: string;
}

/**
 * A row for entering or clearing an item's one-time-code setup.
 *
 * @param props - See {@link TotpFieldProps}.
 */
export function TotpField({ value, onChange, className }: TotpFieldProps) {
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Only the newest parse may write state: pasting a long URI fires several, and
  // an earlier one resolving late would overwrite the answer to the latest input.
  const generation = useRef(0);

  async function commit(raw: string) {
    const trimmed = raw.trim();
    if (trimmed === '') {
      setError(null);
      onChange(null);
      return;
    }
    const mine = ++generation.current;
    setBusy(true);
    try {
      const parsed = await totpParse(trimmed);
      if (mine !== generation.current) return;
      setError(null);
      setText('');
      onChange(parsed);
    } catch (e) {
      if (mine !== generation.current) return;
      const reason = rejectionOf(e);
      setError(reason === null ? 'That setup key could not be read.' : REJECTION[reason]);
    } finally {
      if (mine === generation.current) setBusy(false);
    }
  }

  if (value !== null) {
    const extra = nonDefaults(value);
    return (
      <GroupedRow className={className ?? 'h-[52px]'}>
        <FieldLabel>One-time code</FieldLabel>
        <div className="flex min-w-0 flex-1 flex-col justify-center">
          <span className="text-body text-text-primary font-semibold">
            {value.issuer === '' ? 'Set up' : value.issuer}
            {extra === null ? '' : ` · ${extra}`}
          </span>
          {value.account === '' ? null : (
            <span className="text-caption text-text-muted truncate">{value.account}</span>
          )}
        </div>
        <CopyAction
          className="h-[30px] rounded-md px-[11px]"
          onClick={() => {
            setText('');
            setError(null);
            onChange(null);
          }}
        >
          Remove
        </CopyAction>
      </GroupedRow>
    );
  }

  return (
    <GroupedRow className={className ?? 'h-auto min-h-[52px] py-2'}>
      <FieldLabel>One-time code</FieldLabel>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <Input
          aria-label="One-time code setup key or otpauth link"
          aria-invalid={error !== null}
          className="flex-1 font-mono"
          value={text}
          placeholder="Paste otpauth:// link or setup key"
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          onChange={(event) => {
            setText(event.target.value);
            if (error !== null) setError(null);
          }}
          onBlur={(event) => {
            void commit(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void commit(text);
            }
          }}
          // A paste is the overwhelmingly common case and is unambiguous, so it
          // resolves immediately rather than waiting for the field to lose focus.
          onPaste={(event) => {
            const pasted = event.clipboardData.getData('text');
            if (pasted.trim() !== '') {
              event.preventDefault();
              setText(pasted);
              void commit(pasted);
            }
          }}
        />
        {error === null ? null : (
          <span className="text-caption" data-tone="danger" role="alert">
            {error}
          </span>
        )}
      </div>
      {busy ? <span className="text-caption text-text-muted shrink-0">Reading…</span> : null}
    </GroupedRow>
  );
}
