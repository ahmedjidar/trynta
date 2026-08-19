/**
 * The master-password prompt that `reauthRequired` was always supposed to raise.
 *
 * Two things ask for it (SPEC-V1 §6, §7.5): the rolling 20-reveals-per-minute limit,
 * and the "require the master password to reveal" setting. Both returned
 * `reauthRequired` from Rust, and the UI answered with a toast saying *"confirm your
 * master password"* — which is not a confirmation, it is a description of one. There
 * was no field to type into, so the gate could be triggered and never satisfied. The
 * setting read as broken because, from the user's side, it was.
 *
 * ## The action is retried, not just unblocked
 *
 * A prompt that only clears the flag makes the user find their place again. This
 * takes the thing they were trying to do, holds it, and runs it once the password
 * verifies — so a blocked reveal reveals, and a blocked copy copies. Exactly one
 * retry: the confirmation is spent by it, and pretending otherwise would hand back
 * an allowance the user did not ask for.
 *
 * ## What it does not do
 *
 * It does not remember the password, not even for the length of the retry — the
 * string is passed to `account_reauth` and dropped with the component's state. It
 * does not report *why* a password failed, because there is only one reason worth
 * telling anyone and it is "that was not it".
 */

import { useState } from 'react';

import { Button } from '../../components/Button';
import { Input } from '../../components/Bits';
import { Overlay } from '../../components/Overlay';
import { accountReauth } from '../../ipc';

export interface ReauthPromptProps {
  /**
   * Why the prompt is up, in the user's terms.
   *
   * The two callers mean different things — a setting the user switched on, versus a
   * limit they ran into — and conflating them would make the deliberate one look
   * like an error.
   */
  reason: 'setting' | 'limit' | 'enrol';
  /** What to run again once the password verifies. */
  onConfirmed: () => void;
  /**
   * What to do with the password, when it is not a re-authentication.
   *
   * Defaults to `account_reauth`, which is what the reveal and copy gates want.
   * Enrolling a biometric wants the same box and a different verb — it hands the
   * password to `biometric_enable` — and the alternative was a second component
   * that differed only in which command it called.
   */
  verify?: (password: string) => Promise<unknown>;
  /** Abandon the pending action. */
  onCancel: () => void;
}

/**
 * Ask for the master password, then re-run the action that needed it.
 *
 * @param props - See {@link ReauthPromptProps}.
 */
export function ReauthPrompt({ reason, onConfirmed, onCancel, verify }: ReauthPromptProps) {
  const [password, setPassword] = useState('');
  const [failed, setFailed] = useState(false);
  const [busy, setBusy] = useState(false);

  function submit() {
    if (password === '' || busy) return;
    setBusy(true);
    const check = verify ?? accountReauth;
    check(password).then(
      () => {
        // Cleared before the retry, so the string does not outlive the call.
        setPassword('');
        setBusy(false);
        onConfirmed();
      },
      () => {
        setBusy(false);
        setFailed(true);
        setPassword('');
      },
    );
  }

  return (
    <Overlay onDismiss={onCancel} label="Confirm your master password" placement="centre">
      <div className="bg-surface-panel shadow-sheet w-[420px] rounded-xl p-6">
        <h2 className="text-title text-text-primary font-bold tracking-tight">
          {reason === 'enrol' ? 'Enter your master password' : 'Confirm your master password'}
        </h2>
        <p className="text-body text-text-secondary mt-2">
          {reason === 'setting'
            ? 'You asked Trynta to confirm before a secret leaves the vault.'
            : reason === 'enrol'
              ? 'Trynta stores your master password behind Windows Hello, so it needs it once to set that up.'
              : 'That is a lot of secrets in a short time, so Trynta is checking it is still you.'}
        </p>

        <form
          className="mt-5"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <Input
            autoFocus
            type="password"
            aria-label="Master password"
            aria-invalid={failed}
            className="h-10 w-full"
            value={password}
            placeholder="Master password"
            autoComplete="off"
            onChange={(event) => {
              setPassword(event.target.value);
              if (failed) setFailed(false);
            }}
          />
          {failed ? (
            <p className="text-caption mt-2" data-tone="danger" role="alert">
              That is not your master password.
            </p>
          ) : null}

          <div className="mt-5 flex justify-end gap-2">
            <Button type="button" variant="outline" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={password === '' || busy}>
              {busy ? 'Checking…' : 'Confirm'}
            </Button>
          </div>
        </form>
      </div>
    </Overlay>
  );
}
