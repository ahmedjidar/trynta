/**
 * Item detail — HO-002 `components/ItemDetail.tsx`, SPEC-V1 §7.1, §7.2.
 *
 * ## Edit mode
 *
 * HO-002's Edit button swaps the Username row for an input and the button's label for
 * "Done". The password row is untouched in edit mode, which is the right shape and not an
 * omission: a form pre-filled with the stored password would be a second plaintext path out
 * of Rust, and §4.4 allows exactly one. So edit mode changes **metadata only** — title,
 * username, website, notes — and `item_edit_meta` carries the sealed secret across inside
 * Rust. Setting a new password is a separate, explicit action.
 *
 * ## Autofill
 *
 * HO-002's second header button fires `flash('Autofilled in Safari — …')`. Autofill is
 * SPEC-V3 and there is nothing behind it, so it renders **disabled** with the reason in its
 * tooltip rather than being dropped: the design puts two buttons here, and §7.5's rule is
 * against a control that *appears* to work.
 *
 * ## What is not here
 *
 * HO-002's meta grid pairs "Shared with" — person chips and an "+ Invite" affordance — with
 * "Activity". Sharing is SPEC-V2, so Activity spans the grid alone.
 *
 * ## Reveal
 *
 * The revealed value is derived from held state tagged with the item id rather than reset in
 * an effect, which makes §4.4's "clear on navigation" structural: there is no frame in which
 * the previous item's password is on screen beside the new item's title.
 */

import { useCallback, useEffect, useState } from 'react';

import { Button } from '../../components/Button';
import { CopyAction, Input } from '../../components/Bits';
import {
  Card,
  FieldLabel,
  GroupedList,
  GroupedRow,
  SectionLabel,
} from '../../components/GroupedList';
import { IdentityTile } from '../../components/IdentityTile';
import { StrengthMeter } from '../../components/StrengthMeter';
import { TotpRow } from './TotpRow';
import { useNavigation } from '../../app/navigation';
import { cn } from '../../lib/cn';
import { itemCopyField, itemEditMeta, itemRevealField } from '../../ipc';
import type { ItemDetailDto, ItemSummaryDto, MetaEditsInput, SecretFieldDto } from '../../ipc';

/** HO-002 masks with `'•'.repeat(max(10, length))`; the length is not a hint worth giving. */
const MASK = '•'.repeat(16);

/** Human label per secret field, in the order components.md lists them. */
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

/** Which of the item's own labelled fields edit mode can write. */
const EDITABLE = new Set(['Username', 'Website']);

/** Tone for the strength label, matching HO-002's `strengthColor()` thresholds. */
function strengthTone(band: number): string {
  if (band === 0) return 'empty';
  if (band <= 1) return 'danger';
  if (band === 2) return 'warning';
  return 'accent';
}

export interface ItemDetailProps {
  /** List-row identity: tile, subtitle, TOTP flag. */
  summary: ItemSummaryDto;
  /** Metadata and secret presence, from `item_get`. */
  detail: ItemDetailDto;
  /** Band 0–4 and its label, from the last security report. */
  strength: { band: number; label: string };
  /** Owning vault's name, for the header subtitle. */
  vaultName: string;
  /** Report a copy to the toast. */
  onCopied: (what: string) => void;
  /** Report a failure to the toast. */
  onFailed: (message: string) => void;
  /** Drop the cached list and detail after a successful edit. */
  onEdited: () => void;
}

export function ItemDetail({
  summary,
  detail,
  strength,
  vaultName,
  onCopied,
  onFailed,
  onEdited,
}: ItemDetailProps) {
  const [held, setHeld] = useState<{ itemId: string; key: string; value: string } | null>(null);
  const revealed = held?.itemId === detail.id ? held : null;
  const select = useNavigation((s) => s.select);

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

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

  // Escape leaves edit mode, then closes the pane. HO-002 binds one global handler that
  // closes the palette, leaves edit mode and closes the sheet together; scoping it here
  // means it cannot dismiss an overlay that happens to be open over this pane.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setHeld(null);
      if (editing) {
        setEditing(false);
        setDraft({});
        return;
      }
      select(null);
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
  }, [select, editing]);

  const copy = useCallback(
    (field: SecretFieldDto, what: string) => {
      // Rust reads, decrypts and writes the OS clipboard. The plaintext never enters the
      // webview (CLAUDE.md §4.3).
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

  function save() {
    const edits: MetaEditsInput = {};
    const title = draft.Title;
    if (title !== undefined && title.trim() !== detail.title) edits.title = title.trim();
    const username = draft.Username;
    if (username !== undefined) edits.username = username;
    const website = draft.Website;
    if (website !== undefined) edits.urls = website.trim() === '' ? [] : [website.trim()];
    const notes = draft.Notes;
    if (notes !== undefined && notes !== detail.notes) edits.notes = notes;

    if (Object.keys(edits).length === 0) {
      setEditing(false);
      setDraft({});
      return;
    }

    setSaving(true);
    itemEditMeta(detail.id, edits).then(
      () => {
        setSaving(false);
        setEditing(false);
        setDraft({});
        onEdited();
        onCopied('Changes saved');
      },
      () => {
        setSaving(false);
        onFailed('Could not save the changes');
      },
    );
  }

  const present = detail.secrets.filter((s) => s.present);
  const subtitle = [summary.subtitle, vaultName].filter((part) => part).join(' · ');

  return (
    <section className="bg-surface-panel min-w-0 flex-1 overflow-y-auto" aria-label="Item detail">
      <div className="max-w-[704px] px-8 pt-7 pb-12">
        <header className="flex items-center gap-4">
          <IdentityTile icon={summary.icon} size={56} title={summary.title} />
          <div className="min-w-0 flex-1">
            {editing ? (
              <Input
                aria-label="Title"
                className="text-body-lg h-9 w-full font-semibold"
                value={draft.Title ?? detail.title}
                onChange={(event) => {
                  setDraft((prev) => ({ ...prev, Title: event.target.value }));
                }}
              />
            ) : (
              <h1 className="text-display tracking-display truncate font-bold">{detail.title}</h1>
            )}
            <p className="text-body text-text-caption-aa mt-0.5 truncate">{subtitle}</p>
          </div>
          <div className="flex shrink-0 gap-2">
            <Button
              variant="outline"
              disabled={saving}
              onClick={() => {
                if (editing) save();
                else setEditing(true);
              }}
            >
              {editing ? (saving ? 'Saving…' : 'Done') : 'Edit'}
            </Button>
            <Button disabled title="Autofill arrives in a later version">
              Autofill
            </Button>
          </div>
        </header>

        <GroupedList className="mt-6">
          {/* The item's own non-secret fields — username, cardholder, expiry, whichever
              the kind has. These arrive with the list index, never a secret among them. */}
          {detail.fields.map((field) => (
            <GroupedRow key={field.label} className="h-12">
              <FieldLabel>{field.label}</FieldLabel>
              {editing && EDITABLE.has(field.label) ? (
                <Input
                  aria-label={field.label}
                  className="h-7 flex-1"
                  value={draft[field.label] ?? field.value}
                  onChange={(event) => {
                    setDraft((prev) => ({ ...prev, [field.label]: event.target.value }));
                  }}
                />
              ) : (
                <div
                  className={cn(
                    'text-body min-w-0 flex-1 truncate',
                    field.label === 'Website' ? 'text-accent-text' : 'font-mono',
                  )}
                  data-selectable
                >
                  {/* HO-002 renders the website as an `<a>`. `default-src 'self'` means
                      an external href cannot navigate, and opening the OS browser needs
                      `shell:allow-open`, which this app does not grant — so it would be a
                      link that does nothing. Selectable text instead. */}
                  {field.value}
                </div>
              )}
              <CopyAction
                onClick={() => {
                  navigator.clipboard.writeText(field.value).then(
                    () => {
                      onCopied(`${field.label} copied`);
                    },
                    () => {
                      onFailed('Could not copy');
                    },
                  );
                }}
              >
                Copy
              </CopyAction>
            </GroupedRow>
          ))}

          {present.map((secret) => {
            const key = fieldKey(secret.field);
            const label = SECRET_LABELS[secret.field.field] ?? 'Hidden field';
            const shown = revealed?.key === key;

            if (secret.field.field === 'totpSecret') {
              return (
                <TotpRow
                  key={key}
                  itemId={detail.id}
                  title={detail.title}
                  onCopied={onCopied}
                  onFailed={onFailed}
                />
              );
            }

            return (
              <GroupedRow key={key} className="h-12">
                <FieldLabel>{label}</FieldLabel>
                <div
                  className={cn(
                    'text-body min-w-0 flex-1 overflow-hidden font-mono whitespace-nowrap',
                    shown ? 'tracking-shown' : 'tracking-masked',
                  )}
                  data-selectable={shown ? '' : undefined}
                >
                  {shown ? revealed.value : MASK}
                </div>
                <CopyAction
                  onClick={() => {
                    toggleReveal(secret.field);
                  }}
                >
                  {shown ? 'Hide' : 'Reveal'}
                </CopyAction>
                <CopyAction
                  onClick={() => {
                    copy(secret.field, label);
                  }}
                >
                  Copy
                </CopyAction>
              </GroupedRow>
            );
          })}

          {/* Strength, for a login only: a card PIN has no crack-time band. */}
          {present.some((s) => s.field.field === 'password') ? (
            <GroupedRow className="h-12">
              <FieldLabel>Strength</FieldLabel>
              <StrengthMeter score={strength.band} label={strength.label} />
              <div
                className="text-chip w-[68px] shrink-0 text-right font-bold"
                data-tone={strengthTone(strength.band)}
              >
                {strength.label}
              </div>
            </GroupedRow>
          ) : null}
        </GroupedList>

        <div className="mt-4 grid grid-cols-2 gap-4">
          {/* HO-002 pairs Activity with "Shared with". Sharing is SPEC-V2, so Activity
              spans the grid rather than leaving an empty cell or an invented neighbour. */}
          <Card className="col-span-2 min-h-24">
            <SectionLabel className="h-auto">Activity</SectionLabel>
            <div className="text-caption text-text-secondary mt-3 flex flex-col gap-1.5">
              <div>Created {new Date(detail.createdAt).toLocaleDateString()}</div>
              <div>Revision {detail.revision}</div>
            </div>
          </Card>
        </div>

        <Card className="mt-4">
          <SectionLabel className="h-auto">Notes</SectionLabel>
          {editing ? (
            <textarea
              aria-label="Notes"
              rows={3}
              className="border-strong bg-surface-panel text-body text-text-primary mt-3 w-full resize-none rounded-md border px-2.5 py-2 leading-[18px] outline-none"
              value={draft.Notes ?? detail.notes}
              onChange={(event) => {
                setDraft((prev) => ({ ...prev, Notes: event.target.value }));
              }}
            />
          ) : (
            <p
              className="text-body text-text-secondary mt-3 leading-5 text-pretty whitespace-pre-wrap"
              data-selectable
            >
              {detail.notes === '' ? 'No notes yet.' : detail.notes}
            </p>
          )}
        </Card>
      </div>
    </section>
  );
}
