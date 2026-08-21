// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Lock screen — SPEC-V1 §5.
 *
 * ## Opaque, not a scrim
 *
 * The design is explicit that the vault's contents must not be readable behind this.
 * CLAUDE.md §4.9 says the same from the other direction — lock is real, not a UI overlay —
 * and this goes one step further: the shell is not mounted behind it at all, so there is
 * nothing to read through it even in principle.
 *
 * ## No biometric unlock, and no mention of one
 *
 * The design's copy says "Touch ID" twice and offers a Touch ID row. There is no biometric
 * unlock in this build: `account_unlock_biometric` does not exist and AC06 defers
 * enrolment to the platform secure-store work.
 *
 * An earlier version of this file rendered the row disabled with the reason in a tooltip,
 * and named the platform's sensor in the prose. That was the wrong call and it read as
 * broken: the sentence told the user to unlock with Windows Hello, and the only Hello
 * control on screen said it was not built. A screen that offers a route it cannot take is
 * worse than one that never mentions it. So the prose names the master password only, and
 * the row is gone until the command behind it exists.
 *
 * ## First run
 *
 * On the very first launch this also carries HO-002's pre-unlock notice
 * (`features/tour`). It is a block element in normal flow rather than a floating
 * card — the handoff's own instruction, and the right one here: this surface has
 * nothing else on it for a floating card to be beside. It does not take focus
 * away from the password field.
 *
 * The design has no create-vault screen (recorded in handoffs/MANIFEST.md). Rather than
 * improvise a layout this reuses the lock screen's exactly and adds one confirm row,
 * because a vault created from a single unconfirmed field loses every password to one typo
 * and there is no reset path by design.
 */

import { useEffect, useId, useRef, useState } from 'react';

import { BrandMark } from '../../components/BrandMark';
import { Notice } from '../tour/Notice';
import { Button } from '../../components/Button';
import { IpcError } from '../../ipc';
import { accountCreate, accountUnlock, accountUnlockBiometric, biometricReady } from '../../ipc';
import type { AccountStatus } from '../../ipc';

export interface LockScreenProps {
  /** Whether a vault file exists. False takes the create path. */
  exists: boolean;
  /** Called with the status returned by a successful unlock or create. */
  onUnlocked: (status: AccountStatus) => void;
}

export function LockScreen({ exists, onUnlocked }: LockScreenProps) {
  // Asked, not assumed. The button is only worth drawing when the device has a
  // biometric *and* this vault has been enrolled — either half missing means it
  // cannot work, and an unlock button that cannot unlock is worse than none.
  const [helloReady, setHelloReady] = useState(false);
  const [helloBusy, setHelloBusy] = useState(false);

  useEffect(() => {
    if (!exists) return;
    let live = true;
    biometricReady().then(
      (ready) => {
        if (live) setHelloReady(ready);
      },
      () => {
        // Fail closed: no button rather than one that might not work.
        if (live) setHelloReady(false);
      },
    );
    return () => {
      live = false;
    };
  }, [exists]);
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const passwordId = useId();
  const confirmId = useId();

  useEffect(() => {
    // The design uses the `autoFocus` attribute. Done here so focus also lands after a failed
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
      // A scroll pane, and `my-auto` on the form rather than `items-center` on
      // this. The two look identical while everything fits and differ when it
      // does not: `align-items: center` clips the *top* of an overflowing child,
      // so at the window's own minimum height (620px, `tauri.conf.json`) the mark
      // went off the top and the last line of the first-run card off the bottom.
      // An auto margin resolves to zero in that case and the column simply
      // scrolls. Measured at 940x620 before and after.
      data-scroll-pane
      className="animate-lock-in bg-surface-app absolute inset-0 z-[7] flex justify-center overflow-y-auto"
      role="dialog"
      aria-modal="true"
      aria-labelledby="lock-title"
    >
      <form className="my-auto w-[296px] shrink-0 py-6 text-center" onSubmit={submit}>
        {/* The mark stands on its own here rather than sitting on a coloured tile:
            the lock screen is the one surface with nothing else on it, and a badge
            around a logo is what you do when the logo is a letter. */}
        <div className="brand-mark mx-auto flex h-[60px] items-center justify-center">
          <BrandMark size={92} />
        </div>
        <h1 className="text-title tracking-title mt-5 font-bold" id="lock-title">
          {exists ? 'Vault locked' : 'Create your vault'}
        </h1>
        <p className="text-body text-text-muted mt-2 leading-5 text-pretty">
          {exists
            ? 'Your keys were wiped from memory. Unlock with your master password.'
            : 'This password is the only way into your vault. It is never stored, never sent anywhere, and cannot be reset — if you lose it, the vault is unreadable.'}
        </p>

        {/* The design gives the input a placeholder and no label. A placeholder is not an
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

        {/* Only when the device has a biometric and this vault has been enrolled.
            The design drew a Touch ID row unconditionally and an earlier commit
            removed it for being a promise the build could not keep; this is that
            promise, kept, and it disappears again the moment either half is false. */}
        {helloReady ? (
          <Button
            type="button"
            block
            variant="outline"
            disabled={busy || helloBusy}
            className="text-body mt-2 h-10 rounded-full"
            onClick={() => {
              setHelloBusy(true);
              accountUnlockBiometric().then(
                (status) => {
                  setHelloBusy(false);
                  onUnlocked(status);
                },
                (failure: unknown) => {
                  setHelloBusy(false);
                  // One sentence for every cause. Cancelled, no match and an
                  // invalidated enrolment all mean "use your password", and
                  // separating them would say which attempt got furthest.
                  setError(
                    failure instanceof IpcError && failure.error.kind === 'invalidState'
                      ? 'Use your master password this time — Trynta asks for it every couple of weeks.'
                      : 'Windows Hello did not confirm. Use your master password.',
                  );
                },
              );
            }}
          >
            {helloBusy ? 'Waiting for Windows Hello…' : 'Unlock with Windows Hello'}
          </Button>
        ) : null}

        {/* One live region for both, so a screen reader announces whichever applies
            without the two competing. Reserves its line so the button does not move
            under the pointer when a message appears. */}
        <p
          className="text-chip text-status-danger mt-2 min-h-4 leading-4 text-pretty"
          role="status"
          aria-live="polite"
        >
          {mismatch ? 'Those do not match.' : (error ?? '')}
        </p>

        {/* HO-002's pre-unlock notice, in normal flow after the controls it
            describes. Last in the DOM so Tab reaches its close button after the
            form rather than in the middle of it, and below the error line so a
            message never has to appear between the field and its own card. */}
        <Notice />
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
