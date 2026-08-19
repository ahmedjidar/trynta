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

import { useEffect, useRef, useState } from 'react';

import { CopyAction } from '../../components/Bits';
import { FieldLabel, GroupedRow } from '../../components/GroupedList';
import { totpCopyCurrent, totpCurrent } from '../../ipc';
import type { TotpCodeDto } from '../../ipc';

/**
 * Split a code down the middle, the way the design prints it: `544 798`.
 *
 * Grouping is presentation and is applied here rather than in Rust, so the value that
 * crosses IPC and the value that reaches the clipboard stay exactly what the algorithm
 * produced. Handles 6 and 8 digits, which are the only lengths §4.1 permits.
 */
function grouped(code: string): string {
  const half = Math.ceil(code.length / 2);
  return `${code.slice(0, half)} ${code.slice(half)}`;
}

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
      <GroupedRow className="h-12">
        <FieldLabel>One-time code</FieldLabel>
        <span className="text-body text-text-muted min-w-0 flex-1 truncate">
          Unavailable — the stored configuration is incomplete.
        </span>
      </GroupedRow>
    );
  }

  return (
    <GroupedRow className="h-12">
      <FieldLabel>One-time code</FieldLabel>
      <span
        className="text-secret-lg tracking-otp text-accent shrink-0 font-mono font-semibold whitespace-nowrap"
        data-selectable
      >
        {grouped(code.code)}
      </span>
      {/* The design sets the trough's width inline from `remaining / 30`. That is the one
          genuinely continuous value in the product, and an inline style is dropped under
          the production CSP, so `Countdown` writes it through the CSSOM instead. */}
      <Countdown remaining={remaining} period={code.period} />
      <span className="text-chip text-text-muted w-6 shrink-0 tabular-nums">{remaining}s</span>
      <div className="flex-1" />
      <CopyAction
        onClick={() => {
          // The code, not the seed. See `totpCopyCurrent`.
          totpCopyCurrent(itemId).then(
            () => {
              onCopied('Code copied');
            },
            () => {
              onFailed('Could not copy');
            },
          );
        }}
        aria-label={`Copy the one-time code for ${title}`}
      >
        Copy
      </CopyAction>
    </GroupedRow>
  );
}

/**
 * The draining trough beside the code.
 *
 * Width is set on the element through the CSSOM rather than as a markup `style`
 * attribute: the production CSP is `style-src 'self'`, which drops the attribute but
 * permits a script-driven property write. This is the sanctioned escape hatch for a value
 * that is genuinely continuous — everything with a small closed set of values uses a data
 * attribute and a rule in `theme/dynamic.css` instead.
 */
function Countdown({ remaining, period }: { remaining: number; period: number }) {
  const fill = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const node = fill.current;
    if (!node) return;
    const share = period === 0 ? 0 : Math.max(0, Math.min(1, remaining / period));
    node.style.setProperty('width', `${String(Math.round(share * 100))}%`);
  }, [remaining, period]);

  return (
    <span
      className="bg-strong h-1 w-14 shrink-0 overflow-hidden rounded-sm"
      role="img"
      aria-label={`${String(remaining)} seconds remaining`}
    >
      <span ref={fill} className="countdown__fill block h-full" />
    </span>
  );
}
