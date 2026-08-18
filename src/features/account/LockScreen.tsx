/**
 * Lock screen — HO-002 `overlays/LockScreen.tsx`, SPEC-V1 §5.
 *
 * ## Opaque, not a scrim
 *
 * HO-002's own comment says it: *"Opaque, not a scrim: the vault contents must not be
 * readable behind it."* CLAUDE.md §4.9 says the same from the other direction — lock is
 * real, not a UI overlay — and this goes one step further: the shell is not mounted behind
 * this at all, so there is nothing to read through it even in principle.
 *
 * ## Biometrics
 *
 * HO-002's copy says "Touch ID" twice. CLAUDE.md §6 forbids hardcoding the macOS name of
 * anything the user can see, so both come from `biometric_label` in `account_status` —
 * "Windows Hello" on this platform. Where no sensor is enrolled the affordance is not
 * rendered and the prose does not mention one.
 *
 * The button is present but not wired to a prompt: `account_unlock_biometric` does not
 * exist and AC06 defers enrolment. It renders disabled with the reason stated rather than
 * omitted — a machine with Hello enrolled and no mention of it reads as a bug, and a button
 * that silently does nothing is worse than both.
 *
 * ## First run
 *
 * HO-002 has no create-vault screen (MANIFEST HO-002 item 6). Rather than improvise a
 * layout this reuses its exactly and adds one confirm row, because a vault created from a
 * single unconfirmed field loses every password to one typo and there is no reset path by
 * design.
 */

import { useEffect, useId, useRef, useState } from 'react';

import { Button } from '../../components/Button';
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
    // HO-002 uses the `autoFocus` attribute. Done here so focus also lands after a failed
    // attempt re-renders the form.
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
        // unzeroizable JS string until the engine collects it — SPEC-V1 §2 documents that
        // exposure rather than pretending it away — but holding it in a live component
        // after it has served its purpose is a choice, not a constraint.
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
    <div
      className="animate-lock-in bg-surface-app absolute inset-0 z-[7] flex items-center justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="lock-title"
    >
      <form className="w-[296px] text-center" onSubmit={submit}>
        <div className="bg-accent text-tile-lock text-text-on-accent shadow-accent-glow-lg mx-auto flex h-[60px] w-[60px] items-center justify-center rounded-2xl font-extrabold">
          K
        </div>
        <h1 className="text-title tracking-title mt-5 font-bold" id="lock-title">
          {exists ? 'Vault locked' : 'Create your vault'}
        </h1>
        <p className="text-body text-text-caption-aa mt-2 leading-5 text-pretty">
          {exists
            ? biometricAvailable
              ? `Your keys were wiped from memory. Unlock with ${biometricLabel} or your master password.`
              : 'Your keys were wiped from memory. Unlock with your master password.'
            : 'This password is the only way into your vault. It is never stored, never sent anywhere, and cannot be reset — if you lose it, the vault is unreadable.'}
        </p>

        {/* HO-002 gives the input a placeholder and no label. A placeholder is not an
            accessible name — it disappears on the first keystroke — so the label is
            rendered and visually hidden. The appearance is unchanged. */}
        <label className="sr-only" htmlFor={passwordId}>
          Master password
        </label>
        <input
          id={passwordId}
          ref={field}
          className="border-strong bg-surface-panel text-body text-text-primary mt-6 h-10 w-full rounded-full border px-4 text-center outline-none"
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
            <label className="sr-only" htmlFor={confirmId}>
              Confirm master password
            </label>
            <input
              id={confirmId}
              className="border-strong bg-surface-panel text-body text-text-primary mt-2 h-10 w-full rounded-full border px-4 text-center outline-none"
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

        <Button
          type="submit"
          block
          disabled={!ready || busy}
          className="text-body shadow-accent-glow-lg mt-2 h-10 rounded-full"
        >
          {busy ? (exists ? 'Unlocking…' : 'Creating…') : exists ? 'Unlock' : 'Create vault'}
        </Button>

        {/* One live region for both, so a screen reader announces whichever applies
            without the two competing. Reserves its line so the button does not move
            under the pointer when a message appears. */}
        <p
          className="text-chip text-status-danger-text mt-2 min-h-4 leading-4 text-pretty"
          role="status"
          aria-live="polite"
        >
          {mismatch ? 'Those do not match.' : (error ?? '')}
        </p>

        {exists && biometricAvailable ? (
          <button
            type="button"
            data-focus-ring
            disabled
            // Disabled with the reason in the tooltip rather than in the label: HO-002's
            // row is one line at 296px, and the explanation wrapped it to two.
            title={`${biometricLabel} unlock is not available in this build yet`}
            className="text-control text-text-caption-aa mt-2 flex h-8 w-full items-center justify-center gap-1.5 rounded-full font-semibold"
          >
            <Glyph name="biometric" />
            {`Use ${biometricLabel}`}
          </button>
        ) : null}
      </form>
    </div>
  );
}

/**
 * Turn an IPC failure into a sentence.
 *
 * Deliberately coarse: SPEC-V1 §5 distinguishes a wrong password from a tampered header and
 * the user needs to know which, but nothing here reports *how* a verification failed beyond
 * that.
 */
function describe(cause: unknown, exists: boolean): string {
  if (!(cause instanceof IpcError)) {
    return exists ? 'Could not unlock.' : 'Could not create the vault.';
  }
  switch (cause.error.kind) {
    case 'wrongPassword':
      return 'That password does not match.';
    case 'backoff': {
      // The variant carries the wait, so state it. "Try again later" invites the retry
      // that resets the backoff and makes the wait longer.
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
