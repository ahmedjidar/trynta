/**
 * Lock screen — components.md §14, SPEC-V1 §5.
 *
 * ## Why this is opaque and not an overlay
 *
 * §14 specifies *"opaque `--surface-app` (not a scrim)"*, and CLAUDE.md §4.9 says the same
 * thing from the other direction: *"Lock is real. It is not a UI overlay."* A translucent
 * veil over a rendered vault would leave item titles legible through the blur, which is an
 * inventory of the vault visible to anyone at the machine. So this replaces the shell
 * rather than covering it, and the shell is not mounted at all while locked.
 *
 * ## Biometrics
 *
 * The design's copy says "Touch ID" twice. CLAUDE.md §6 forbids hardcoding the macOS name
 * of anything the user can see, so both come from `biometricLabel` in `account_status` —
 * "Windows Hello" on this platform. When no sensor is enrolled the affordance is not
 * rendered and the prose does not mention one, because §7.5's rule against *"a toggle that
 * does nothing"* applies at least as strongly to the one control standing between the user
 * and their vault.
 *
 * The button is present but not yet wired to a prompt: `account_unlock_biometric` does not
 * exist, and AC06 defers biometric enrolment. It is rendered disabled with the reason
 * stated, rather than omitted — a machine with Hello enrolled and no mention of it reads as
 * a bug, and a button that silently does nothing is worse than both.
 *
 * ## First run
 *
 * The design has no create-vault screen (HO-002 item 6). Rather than improvise a layout,
 * this reuses §14's exactly and adds one confirm row, because the alternative — a vault
 * created from a single unconfirmed field — loses every password to one typo, and there is
 * no reset path by design.
 */

import { useEffect, useId, useRef, useState } from 'react';

import { Button } from '../../components/Controls';
import { Glyph } from '../../components/Glyph';
import { IpcError } from '../../ipc';
import { accountCreate, accountUnlock } from '../../ipc';
import type { AccountStatus } from '../../ipc';

export interface LockScreenProps {
  /** Whether a vault file exists. False takes the create path. */
  exists: boolean;
  /** Platform biometric name, e.g. "Windows Hello". Empty when none is available. */
  biometricLabel: string;
  /** Whether the platform reports an enrolled sensor. */
  biometricAvailable: boolean;
  /** Called with the status returned by a successful unlock or create. */
  onUnlocked: (status: AccountStatus) => void;
}

export function LockScreen({
  exists,
  biometricLabel,
  biometricAvailable,
  onUnlocked,
}: LockScreenProps) {
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const passwordId = useId();
  const confirmId = useId();

  useEffect(() => {
    // §14: the input is autofocused. Done here rather than with the `autoFocus`
    // attribute so it also lands after a failed attempt re-renders the form.
    field.current?.focus();
  }, []);

  const mismatch = !exists && confirm.length > 0 && confirm !== password;
  const ready = password.length > 0 && (exists || (confirm === password && confirm.length > 0));

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!ready || busy) return;
    setBusy(true);
    setError(null);

    const attempt = exists ? accountUnlock(password) : accountCreate(password);
    attempt.then(
      (status) => {
        // Drop the plaintext from React state before anything else. It stays an
        // unzeroizable JS string until the engine collects it — SPEC-V1 §2 documents
        // that exposure rather than pretending it away — but holding it in a live
        // component after it has served its purpose is a choice, not a constraint.
        setPassword('');
        setConfirm('');
        setBusy(false);
        onUnlocked(status);
      },
      (cause: unknown) => {
        setBusy(false);
        setPassword('');
        setError(describe(cause, exists));
        field.current?.focus();
      },
    );
  }

  return (
    <div className="lock" role="dialog" aria-modal="true" aria-labelledby="lock-title">
      <form className="lock__card" onSubmit={submit}>
        <span className="lock__mark" aria-hidden="true">
          K
        </span>
        <h1 className="lock__title" id="lock-title">
          {exists ? 'Vault locked' : 'Create your vault'}
        </h1>
        <p className="lock__prose">
          {exists
            ? biometricAvailable
              ? `Your keys were wiped from memory. Unlock with ${biometricLabel} or your master password.`
              : 'Your keys were wiped from memory. Unlock with your master password.'
            : 'This password is the only way into your vault. It is never stored, never sent anywhere, and cannot be reset — if you lose it, the vault is unreadable.'}
        </p>

        <label className="lock__label" htmlFor={passwordId}>
          Master password
        </label>
        <input
          id={passwordId}
          ref={field}
          className="lock__input"
          type="password"
          value={password}
          autoComplete={exists ? 'current-password' : 'new-password'}
          placeholder="Master password"
          disabled={busy}
          onChange={(event) => {
            setPassword(event.target.value);
          }}
        />

        {exists ? null : (
          <>
            <label className="lock__label" htmlFor={confirmId}>
              Confirm master password
            </label>
            <input
              id={confirmId}
              className="lock__input"
              type="password"
              value={confirm}
              autoComplete="new-password"
              placeholder="Type it again"
              disabled={busy}
              onChange={(event) => {
                setConfirm(event.target.value);
              }}
            />
          </>
        )}

        <div className="lock__action">
          <Button variant="primary" type="submit" disabled={!ready || busy} block>
            {busy ? (exists ? 'Unlocking…' : 'Creating…') : exists ? 'Unlock' : 'Create vault'}
          </Button>
        </div>

        {/* One live region for both, so a screen reader announces whichever applies
            without the two competing. */}
        <p className="lock__message" role="status" aria-live="polite">
          {mismatch ? 'Those do not match.' : (error ?? '')}
        </p>

        {exists && biometricAvailable ? (
          <button
            type="button"
            className="lock__biometric"
            // No command to call yet. Stating why beats a button that appears to
            // work, and beats hiding a sensor the user knows they have.
            disabled
            title={`${biometricLabel} unlock is not wired up yet`}
          >
            <Glyph name="biometric" />
            {`Use ${biometricLabel} — not available in this build`}
          </button>
        ) : null}
      </form>
    </div>
  );
}

/**
 * Turn an IPC failure into a sentence.
 *
 * Deliberately coarse on the unlock path: SPEC-V1 §5 distinguishes a wrong password from
 * a tampered header, and the user needs to know which, but nothing here reports *how* a
 * verification failed beyond that.
 */
function describe(cause: unknown, exists: boolean): string {
  if (!(cause instanceof IpcError)) {
    return exists ? 'Could not unlock.' : 'Could not create the vault.';
  }
  switch (cause.error.kind) {
    case 'wrongPassword':
      return 'That password does not match.';
    case 'backoff': {
      // The variant carries the wait, so state it. "Try again later" invites the
      // retry that resets the backoff and makes the wait longer.
      const seconds = cause.error.retryInSeconds;
      return `Too many attempts. Try again in ${String(Math.ceil(seconds))} seconds.`;
    }
    case 'tamperDetected':
      // Fail closed and say so plainly: this is not a retry-able state.
      return 'The vault file failed its integrity check and was not opened.';
    case 'noVault':
      return 'No vault file was found.';
    case 'invalidState':
      return exists ? 'The vault is not locked.' : 'A vault already exists on this device.';
    default:
      return exists ? 'Could not unlock.' : 'Could not create the vault.';
  }
}
