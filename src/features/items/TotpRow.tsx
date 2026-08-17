/**
 * The one-time code row — components.md §6, SPEC-V1 §7.2.
 *
 * §7.2 asks for a *"live TOTP with countdown"*. Two things about how that is done here:
 *
 * **The code is a secret and the seed is a worse one.** `totp_current` returns the
 * six-or-eight digit code and its countdown — never the seed. The seed stays in Rust
 * behind `item_totp`, which is only called to compute a code. So the webview holds a
 * value that expires in seconds rather than one that works forever.
 *
 * **The countdown is derived, not polled.** Polling once a second would mean an IPC
 * round trip per second per open item, and would still drift. Instead the code is
 * fetched once, its `secondsRemaining` starts a local timer, and a refetch happens when
 * the timer reaches zero. One request per period.
 */

import { useEffect, useState } from 'react';

import { CopyAction, GroupRow } from '../../components/Controls';
import { itemCopyField, totpCurrent } from '../../ipc';
import type { TotpCodeDto } from '../../ipc';

export interface TotpRowProps {
  itemId: string;
  title: string;
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

export function TotpRow({ itemId, title, onCopied, onFailed }: TotpRowProps) {
  const [code, setCode] = useState<TotpCodeDto | null>(null);
  const [remaining, setRemaining] = useState(0);

  // Fetch on mount and whenever the period rolls over.
  useEffect(() => {
    let live = true;
    // Named `refresh`, not `fetch`: shadowing the network API inside a file whose
    // entire point is that it makes no network request is confusing to read, and
    // `check:network` flagged it — correctly. The guard is right; the name was wrong.
    const refresh = () => {
      totpCurrent(itemId).then(
        (next) => {
          if (!live) return;
          setCode(next);
          setRemaining(next.secondsRemaining);
        },
        () => {
          if (!live) return;
          // §4.1: a seed stored without its parameters returns notFound rather than
          // guessing SHA-1/6/30, because a plausible wrong code is worse than none.
          setCode(null);
        },
      );
    };
    refresh();

    const tick = setInterval(() => {
      setRemaining((left) => {
        if (left <= 1) {
          refresh();
          return 0;
        }
        return left - 1;
      });
    }, 1000);

    return () => {
      live = false;
      clearInterval(tick);
    };
  }, [itemId]);

  if (!code) {
    return (
      <GroupRow>
        <span className="field-label">One-time code</span>
        <span className="field-value">Unavailable — the stored configuration is incomplete.</span>
      </GroupRow>
    );
  }

  // The trough drains left to right over the period. Width is a percentage of a fixed
  // 56px column, so it is arithmetic rather than a token, and it is set through a data
  // attribute bucket rather than the banned `style` prop.
  const decile = Math.max(0, Math.min(10, Math.round((remaining / code.period) * 10)));

  return (
    <GroupRow>
      <span className="field-label">One-time code</span>
      <span className="otp" data-selectable>
        {code.code}
      </span>
      <span className="otp-trough" role="img" aria-label={`${String(remaining)} seconds remaining`}>
        <span className="otp-trough__fill" data-decile={decile} />
      </span>
      <span className="otp-count">{remaining}s</span>
      <span className="detail-spacer" />
      <CopyAction
        onClick={() => {
          itemCopyField(itemId, { field: 'totpSecret' }).then(
            () => {
              onCopied('Code copied');
            },
            () => {
              onFailed('Could not copy');
            },
          );
        }}
        label={`Copy the one-time code for ${title}`}
      >
        Copy
      </CopyAction>
    </GroupRow>
  );
}
