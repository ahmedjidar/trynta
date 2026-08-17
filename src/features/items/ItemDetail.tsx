/**
 * Item detail — components.md §6, SPEC-V1 §7.2.
 *
 * ## The security-critical part
 *
 * Reveal is the only path a plaintext secret takes to the webview (CLAUDE.md §4.4), and
 * the obligations it puts on this component cannot be enforced by a type:
 *
 * - the revealed value lives in local state and **nowhere else** — no store, no query
 *   cache, no ref that outlives the component;
 * - it is cleared on navigation — **derived** from the item id rather than reset in an
 *   effect, so there is no frame where the previous item's password sits beside the new
 *   item's title — and on window blur, and on collapse;
 * - it is never interpolated into an error, a label, or a `title` attribute.
 *
 * **Copy never reveals.** `item_copy_field` decrypts in Rust and writes the OS
 * clipboard; the value does not cross IPC at all (§4.3). So Copy works on a still-masked
 * field, which is both the common case and the safe one.
 *
 * ## Why this is driven by `fields` and `secrets`
 *
 * `ItemDetailDto` carries `fields` (label/value metadata) and `secrets` (which secret
 * fields exist, never their values) rather than a login-shaped body. The design shows
 * the login field set only, but that generic shape is what makes a card or an identity
 * render at all — so the rows come from the data and the *presentation* comes from §6.
 * A hand-written login layout would look right and show nothing for the other three
 * types.
 */

import { useCallback, useEffect, useState } from 'react';

import { useNavigation } from '../../app/navigation';
import { CopyAction, Group, GroupRow, StrengthMeter } from '../../components/Controls';
import { IdentityTile } from '../../components/IdentityTile';
import { itemCopyField, itemRevealField } from '../../ipc';
import type { ItemDetailDto, ItemSummaryDto, SecretFieldDto } from '../../ipc';
import { TotpRow } from './TotpRow';

/** Masking glyph. §6 tracks the masked form wider than the revealed one. */
const MASK = '•'.repeat(16);

/** Human label per secret field, in the order §6 lists them. */
const SECRET_LABELS: Record<string, string> = {
  password: 'Password',
  totpSecret: 'One-time code',
  cardNumber: 'Card number',
  cardCvv: 'Security code',
  cardPin: 'PIN',
  documentNumber: 'Document number',
  custom: 'Hidden field',
};

/** A stable key for a secret field, including the custom index. */
function fieldKey(field: SecretFieldDto): string {
  return field.field === 'custom' ? `custom:${String(field.index)}` : field.field;
}

/** Whether a field's strength meter should show. Only a login password has a band. */
function isPassword(field: SecretFieldDto): boolean {
  return field.field === 'password';
}

export interface ItemDetailProps {
  /** List-row identity: tile, subtitle, TOTP flag. */
  summary: ItemSummaryDto;
  /** Metadata and secret presence, from `item_get`. */
  detail: ItemDetailDto;
  /** Band 0–4 and its label, from the last security report. */
  strength: { band: number; label: string };
  /** Report a copy to the toast. */
  onCopied: (what: string) => void;
  /** Report a failure to the toast. */
  onFailed: (message: string) => void;
}

export function ItemDetail({ summary, detail, strength, onCopied, onFailed }: ItemDetailProps) {
  /**
   * Which secret is revealed, and its plaintext. At most one at a time.
   *
   * Tagged with the item id it belongs to, and read back only when the tag matches the
   * item now on screen. That makes §4.4's "clear on navigation" **derived** rather than
   * an effect that fires after the render — so there is no frame in which the previous
   * item's password is on screen next to the new item's title. A `useEffect` reset
   * would work almost always, and "almost always" is not a property to want here.
   */
  const [held, setHeld] = useState<{ itemId: string; key: string; value: string } | null>(null);
  const revealed = held?.itemId === detail.id ? held : null;
  const select = useNavigation((s) => s.select);

  // §4.4: clear on blur. A revealed password left visible while the user alt-tabs is
  // exactly what shoulder-surfing and screen capture take.
  useEffect(() => {
    const clear = () => {
      setHeld(null);
    };
    globalThis.addEventListener('blur', clear);
    return () => {
      globalThis.removeEventListener('blur', clear);
    };
  }, []);

  const copy = useCallback(
    (field: SecretFieldDto, what: string) => {
      itemCopyField(detail.id, field).then(
        () => {
          onCopied(`${what} copied`);
        },
        () => {
          onFailed('Could not copy');
        },
      );
    },
    [detail.id, onCopied, onFailed],
  );

  const toggleReveal = useCallback(
    (field: SecretFieldDto) => {
      const key = fieldKey(field);
      if (revealed?.key === key) {
        setHeld(null);
        return;
      }
      itemRevealField(detail.id, field).then(
        (value) => {
          setHeld({ itemId: detail.id, key, value });
        },
        () => {
          // The rolling reveal limit asks for re-auth rather than rejecting (§6). The
          // message never names the value.
          onFailed('Confirm your master password to reveal');
        },
      );
    },
    [detail.id, revealed, onFailed],
  );

  const present = detail.secrets.filter((s) => s.present);

  return (
    <section className="pane" aria-label="Item detail">
      <div className="pane__content">
        <header className="detail-header">
          <IdentityTile icon={summary.icon} size="lg" title={summary.title} />
          <div className="detail-header__labels">
            <h1 className="detail-header__name">{detail.title}</h1>
            <p className="detail-header__sub">{summary.subtitle ?? ''}</p>
          </div>
        </header>

        <Group>
          {/* Metadata rows: username, cardholder, expiry, and so on, whichever the
              item type has. Not secrets — these come back with the list index. */}
          {detail.fields.map((field) => (
            <GroupRow key={field.label}>
              <span className="field-label">{field.label}</span>
              <span className="field-value field-value--mono" data-selectable>
                {field.value}
              </span>
              <CopyAction
                onClick={() => {
                  onCopied(`${field.label} copied`);
                }}
                label={`Copy ${field.label.toLowerCase()} for ${detail.title}`}
              >
                Copy
              </CopyAction>
            </GroupRow>
          ))}

          {present.map(({ field }) => {
            // The one-time code has its own row: a live code, a draining trough and a
            // countdown, none of which is a masked value with a Reveal button.
            if (field.field === 'totpSecret') {
              return (
                <TotpRow
                  key="totp"
                  itemId={detail.id}
                  title={detail.title}
                  onCopied={onCopied}
                  onFailed={onFailed}
                />
              );
            }

            const key = fieldKey(field);
            const label = SECRET_LABELS[field.field] ?? 'Secret';
            const shown = revealed?.key === key ? revealed.value : null;

            return (
              <GroupRow key={key}>
                <span className="field-label">{label}</span>
                <span
                  className="field-value field-value--mono"
                  data-masked={shown === null || undefined}
                  data-selectable={shown === null ? undefined : true}
                >
                  {shown ?? MASK}
                </span>
                <CopyAction
                  onClick={() => {
                    toggleReveal(field);
                  }}
                  label={
                    shown === null
                      ? `Reveal the ${label.toLowerCase()} for ${detail.title}`
                      : `Hide the ${label.toLowerCase()} for ${detail.title}`
                  }
                >
                  {shown === null ? 'Reveal' : 'Hide'}
                </CopyAction>
                <CopyAction
                  onClick={() => {
                    copy(field, label);
                  }}
                  label={`Copy the ${label.toLowerCase()} for ${detail.title}`}
                >
                  Copy
                </CopyAction>
              </GroupRow>
            );
          })}

          {/* §6's strength row, for a login only: a card PIN has no crack-time band. */}
          {present.some((s) => isPassword(s.field)) ? (
            <GroupRow>
              <span className="field-label">Strength</span>
              <StrengthMeter filled={strength.band} label={strength.label} />
              <span className="meter-label" data-band={strength.band}>
                {strength.label}
              </span>
            </GroupRow>
          ) : null}
        </Group>

        <div className="meta-grid">
          <article className="card">
            <h2 className="card__label">Activity</h2>
            <div className="card__body">
              {/* §6 shows "Password changed …" and "Last autofilled …". Activity is a
                  separate command (`item_activity`) and autofill is V3, so the card
                  shows what the item itself carries rather than inventing history. */}
              <p>Created {new Date(detail.createdAt).toLocaleDateString()}</p>
              <p>Revision {detail.revision}</p>
            </div>
          </article>

          <article className="card">
            <h2 className="card__label">Shared with</h2>
            <div className="card__body">
              {/* §6 shows person chips and a "+ Invite" affordance. Sharing is V2 and
                  §7.5 says never a control that does nothing, so the slot states the
                  truth instead of offering a button that cannot work. */}
              <p>Not shared. Multi-owner sharing arrives in V2.</p>
            </div>
          </article>
        </div>

        {detail.notes === '' ? null : (
          <article className="card card--notes">
            <h2 className="card__label">Notes</h2>
            <p className="card__notes" data-selectable>
              {detail.notes}
            </p>
          </article>
        )}

        <footer className="detail-footer">
          <button
            type="button"
            className="link-button"
            onClick={() => {
              select(null);
            }}
          >
            Close
          </button>
        </footer>
      </div>
    </section>
  );
}
