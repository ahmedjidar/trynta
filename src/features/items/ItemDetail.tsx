/**
 * Item detail — SPEC-V1 §7.1, §7.2.
 *
 * Header, field group, meta cards, notes. The field order is the design's: the identifier
 * first, then the password with its strength directly under it, then the one-time code,
 * and the website last.
 *
 * ## Edit mode
 *
 * The Edit button swaps the editable rows for inputs and its own label for "Done". The
 * password row is untouched, which is the design's shape and not an omission: a form
 * pre-filled with the stored password would be a second plaintext path out of Rust, and
 * §4.4 allows exactly one. So edit mode changes **metadata only** — title, username,
 * website, notes — and `item_edit_meta` carries the sealed secret across inside Rust.
 * Setting a new password is a separate, explicit action.
 *
 * ## The header's primary action
 *
 * The design's second header button is Autofill. Autofill is SPEC-V3 and there is nothing
 * behind it, and §7.5 is explicit that a control which appears to work and does not is
 * worse than none. The button keeps its place and its treatment and does the thing
 * autofill would be a shortcut for: it copies the item's primary secret, in Rust, without
 * the value entering the webview.
 *
 * ## Reveal
 *
 * The revealed value is derived from held state tagged with the item id rather than reset
 * in an effect, which makes §4.4's "clear on navigation" structural: there is no frame in
 * which the previous item's password is on screen beside the new item's title.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Dispatch, SetStateAction } from 'react';

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
import { useItemIcon } from './useItemIcons';
import { useThemeStore } from '../../theme/store';
import { StrengthMeter } from '../../components/StrengthMeter';
import { TotpRow } from './TotpRow';
import { useNavigation } from '../../app/navigation';
import { cn } from '../../lib/cn';
import {
  itemClearIcon,
  itemCopyField,
  itemEditMeta,
  itemRevealField,
  itemSetIcon,
} from '../../ipc';
import type {
  ItemDetailDto,
  ItemSummaryDto,
  LabelledValue,
  MetaEditsInput,
  SecretFieldDto,
  SecretPresence,
} from '../../ipc';

/** Masked to a fixed width: the stored length is not a hint worth giving away. */
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

/**
 * The primary secret per item kind (§7.1): what ⌘C copies without opening the item, and
 * what the header's copy action reaches for.
 */
const PRIMARY_SECRET: Record<string, { field: SecretFieldDto; label: string }> = {
  login: { field: { field: 'password' }, label: 'Copy password' },
  card: { field: { field: 'cardNumber' }, label: 'Copy number' },
  identity: { field: { field: 'documentNumber' }, label: 'Copy number' },
};

/** A stable key for a secret field, including the custom index. */
function fieldKey(field: SecretFieldDto): string {
  return field.field === 'custom' ? `custom:${String(field.index)}` : field.field;
}

/** Which of the item's own labelled fields edit mode can write. */
const EDITABLE = new Set(['Username', 'Website']);

/** Tone for the strength label, on the design's own thresholds. */
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
  const [iconBusy, setIconBusy] = useState(false);

  const isCustomIcon = summary.icon.kind === 'custom';
  const customSrc = useItemIcon(detail.id, isCustomIcon);
  const resolvedTheme = useThemeStore((s) => s.resolved);

  /**
   * Attach or remove the item's own icon (ADD-001 tier 2).
   *
   * `itemSetIcon` opens the dialog and does the whole pipeline in Rust; the webview
   * never sees the file. `null` back means the user cancelled, which is not an error and
   * must not produce a toast.
   */
  function pickIcon() {
    setIconBusy(true);
    itemSetIcon(detail.id).then(
      (result) => {
        setIconBusy(false);
        if (result === null) return;
        onEdited();
        onCopied(`Icon set, ${String(Math.round(result.bytes / 1024))} KB`);
      },
      () => {
        setIconBusy(false);
        // One message for every rejection. The Rust side deliberately does not report
        // *which* rule refused the file, so this states what is accepted instead.
        onFailed('That file was not accepted. Use an SVG, PNG, JPEG, WebP or ICO under 2 MB.');
      },
    );
  }

  function clearIcon() {
    setIconBusy(true);
    itemClearIcon(detail.id).then(
      () => {
        setIconBusy(false);
        onEdited();
        onCopied('Icon removed');
      },
      () => {
        setIconBusy(false);
        onFailed('Could not remove the icon');
      },
    );
  }

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

  // Escape leaves edit mode, then closes the pane. Scoped here rather than global, so it
  // cannot dismiss an overlay that happens to be open over this pane.
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

  /**
   * The design's row order: identifier, password, strength, one-time code, website.
   *
   * Rust emits the item's own fields in its own order and the secrets separately, so the
   * two are interleaved here rather than rendered one list after the other — a password
   * three rows below the strength that describes it is a different composition.
   */
  const rows = useMemo(() => {
    const present = detail.secrets.filter((s) => s.present);
    const secretLabels = new Set(present.map((s) => SECRET_LABELS[s.field.field]));

    const leading: LabelledValue[] = [];
    const trailing: LabelledValue[] = [];
    for (const field of detail.fields) {
      // A card's last four digits arrive as a non-secret field *and* as the masked
      // secret, so rendering both puts two "Card number" rows in the group. The secret
      // row carries Reveal and Copy, so it is the one that survives.
      if (secretLabels.has(field.label)) continue;
      (field.label === 'Website' ? trailing : leading).push(field);
    }

    const password = present.find((s) => s.field.field === 'password');
    const totp = present.find((s) => s.field.field === 'totpSecret');
    const others = present.filter((s) => s !== password && s !== totp);
    return { leading, trailing, password, totp, others, present };
  }, [detail.fields, detail.secrets]);

  const primary = PRIMARY_SECRET[detail.kind];
  const canCopyPrimary =
    primary !== undefined && rows.present.some((s) => s.field.field === primary.field.field);
  const subtitle = [summary.subtitle, vaultName].filter((part) => part).join(' · ');

  return (
    <section
      data-scroll-pane
      className="bg-surface-panel animate-pane-in min-w-0 flex-1 overflow-x-hidden overflow-y-auto"
      aria-label="Item detail"
    >
      <div className="mx-auto w-full max-w-[var(--measure-pane-wide)] px-8 pt-8 pb-12">
        <header className="flex items-center gap-4">
          <IdentityTile
            icon={summary.icon}
            size={56}
            title={summary.title}
            customSrc={customSrc}
            theme={resolvedTheme}
          />
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
            <p className="text-body text-text-muted mt-0.5 truncate">{subtitle}</p>

            {/* The icon controls live in edit mode rather than behind a hover on the
                tile: an affordance nobody can see is an affordance nobody uses, and
                attaching an icon is an edit. Nothing here is required — almost every
                item resolves to a bundled brand mark on its own. */}
            {editing ? (
              <div className="mt-2 flex items-center gap-2">
                <CopyAction onClick={pickIcon} disabled={iconBusy}>
                  {isCustomIcon ? 'Change icon' : 'Use my own icon'}
                </CopyAction>
                {isCustomIcon ? (
                  <CopyAction onClick={clearIcon} disabled={iconBusy}>
                    Remove
                  </CopyAction>
                ) : null}
                <span className="text-chip text-text-muted">SVG, PNG, JPEG, WebP or ICO</span>
              </div>
            ) : null}
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
            {primary && canCopyPrimary ? (
              <Button
                onClick={() => {
                  copy(primary.field, SECRET_LABELS[primary.field.field] ?? 'Value');
                }}
              >
                {primary.label}
              </Button>
            ) : null}
          </div>
        </header>

        <GroupedList className="mt-6">
          {rows.leading.map((field) => (
            <FieldRow
              key={field.label}
              field={field}
              editing={editing}
              draft={draft}
              setDraft={setDraft}
              onCopied={onCopied}
              onFailed={onFailed}
            />
          ))}

          {rows.password ? (
            <SecretRow
              secret={rows.password}
              revealedKey={revealed?.key ?? null}
              revealedValue={revealed?.value ?? ''}
              onToggle={toggleReveal}
              onCopy={copy}
            />
          ) : null}

          {rows.password ? (
            <GroupedRow className="h-12">
              <FieldLabel>Strength</FieldLabel>
              <StrengthMeter score={strength.band} label={strength.label} />
              <div
                className="text-chip min-w-[68px] shrink-0 text-right font-bold whitespace-nowrap"
                data-tone={strengthTone(strength.band)}
              >
                {strength.label}
              </div>
            </GroupedRow>
          ) : null}

          {rows.totp ? (
            <TotpRow
              itemId={detail.id}
              title={detail.title}
              onCopied={onCopied}
              onFailed={onFailed}
            />
          ) : null}

          {rows.others.map((secret) => (
            <SecretRow
              key={fieldKey(secret.field)}
              secret={secret}
              revealedKey={revealed?.key ?? null}
              revealedValue={revealed?.value ?? ''}
              onToggle={toggleReveal}
              onCopy={copy}
            />
          ))}

          {rows.trailing.map((field) => (
            <FieldRow
              key={field.label}
              field={field}
              editing={editing}
              draft={draft}
              setDraft={setDraft}
              onCopied={onCopied}
              onFailed={onFailed}
            />
          ))}
        </GroupedList>

        <div className="mt-4 grid grid-cols-2 gap-4">
          {/* The design pairs Activity with "Shared with". Sharing is SPEC-V2, so Activity
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

interface FieldRowProps {
  field: LabelledValue;
  editing: boolean;
  draft: Record<string, string>;
  setDraft: Dispatch<SetStateAction<Record<string, string>>>;
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

/** One of the item's own non-secret fields. Never a secret among them. */
function FieldRow({ field, editing, draft, setDraft, onCopied, onFailed }: FieldRowProps) {
  return (
    <GroupedRow className="h-12">
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
            field.label === 'Website' ? 'text-accent' : 'font-mono',
          )}
          data-selectable
        >
          {/* The design renders the website as an `<a>`. `default-src 'self'` means an
              external href cannot navigate, and opening the OS browser needs
              `shell:allow-open`, which this app does not grant — so it would be a link
              that does nothing. Selectable text in the accent instead. */}
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
  );
}

interface SecretRowProps {
  secret: SecretPresence;
  revealedKey: string | null;
  revealedValue: string;
  onToggle: (field: SecretFieldDto) => void;
  onCopy: (field: SecretFieldDto, what: string) => void;
}

/** A masked secret with Reveal and Copy. The value is only ever held by the caller. */
function SecretRow({ secret, revealedKey, revealedValue, onToggle, onCopy }: SecretRowProps) {
  const key = fieldKey(secret.field);
  const label = SECRET_LABELS[secret.field.field] ?? 'Hidden field';
  const shown = revealedKey === key;

  return (
    <GroupedRow className="h-12">
      <FieldLabel>{label}</FieldLabel>
      <div
        className={cn(
          'text-body min-w-0 flex-1 overflow-hidden font-mono whitespace-nowrap',
          shown ? 'tracking-shown' : 'tracking-masked',
        )}
        data-selectable={shown ? '' : undefined}
      >
        {shown ? revealedValue : MASK}
      </div>
      <CopyAction
        onClick={() => {
          onToggle(secret.field);
        }}
      >
        {shown ? 'Hide' : 'Reveal'}
      </CopyAction>
      <CopyAction
        onClick={() => {
          onCopy(secret.field, label);
        }}
      >
        Copy
      </CopyAction>
    </GroupedRow>
  );
}
