/**
 * New-item sheet — components.md §14, SPEC-V1 §7.1.
 *
 * ## Structure, from §14
 *
 * Header (56px, accent tile, title, subtitle) · 4-up kind segmented control that swaps the
 * field set · 64px preview tile with a monogram built from the typed name · name input ·
 * grouped field rows per kind · vault chips · notes · footer whose copy states where the
 * item will be saved and whose Save button is gated until the item has a name.
 *
 * ## Three places this departs from the design, and why
 *
 * - **The subtitle.** The design reads "Encrypted on this Mac before it syncs." Two
 *   problems: ADD-005 makes Windows the platform, so "Mac" is wrong here, and sync is
 *   SPEC-V3 — there is nothing to sync to, so the sentence promises a feature that does
 *   not exist. Rewritten to say what actually happens.
 * - **"Ask for Touch ID before autofill".** Autofill is V3 (§7.5) and there is no field to
 *   store this against, so the row is not drawn. §7.5: never *"a toggle that does
 *   nothing"*.
 * - **The favicon crossfade.** §14 has one fade in "once the domain parses". ADD-001 bans
 *   fetching icons, so the preview tile shows the monogram the item will actually get,
 *   which is what Rust will resolve for it.
 *
 * All three are HO-002 items in `handoffs/MANIFEST.md`.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { Button, Group, GroupRow, Segmented, StrengthMeter } from '../../components/Controls';
import type { SegmentedOption } from '../../components/Controls';
import { Glyph } from '../../components/Glyph';
import { IdentityTile } from '../../components/IdentityTile';
import { generatorPassword, itemUpsert, passwordStrength } from '../../ipc';
import type { ItemBodyInput, ItemKindDto, StrengthDto, VaultSummaryDto } from '../../ipc';

/** The four kinds §14's segmented control offers, in its order. */
const KINDS: readonly SegmentedOption<ItemKindDto>[] = [
  { value: 'login', label: 'Login', glyph: <Glyph name="login" /> },
  { value: 'secureNote', label: 'Note', glyph: <Glyph name="note" /> },
  { value: 'card', label: 'Card', glyph: <Glyph name="card" /> },
  { value: 'identity', label: 'Identity', glyph: <Glyph name="identity" /> },
];

/** Placeholder per kind for the name field, as §14's `newNamePh` does. */
const NAME_PLACEHOLDER: Record<ItemKindDto, string> = {
  login: 'Northwind Mail',
  secureNote: 'Recovery codes',
  card: 'Everyday card',
  identity: 'Passport',
};

/** Every text field any kind needs, keyed so one state object serves all four. */
type FieldKey =
  | 'username'
  | 'password'
  | 'website'
  | 'cardholder'
  | 'number'
  | 'expiryMonth'
  | 'expiryYear'
  | 'cvv'
  | 'pin'
  | 'billingAddress'
  | 'firstName'
  | 'lastName'
  | 'dob'
  | 'documentType'
  | 'documentNumber'
  | 'issuingCountry'
  | 'expiry'
  | 'address'
  | 'phone'
  | 'email';

interface FieldSpec {
  key: FieldKey;
  label: string;
  placeholder?: string;
  mono?: boolean;
}

/** The field set per kind — §14's "swaps the field set below". */
const FIELDS: Record<ItemKindDto, readonly FieldSpec[]> = {
  login: [
    { key: 'username', label: 'Username', placeholder: 'you@example.com' },
    { key: 'password', label: 'Password', placeholder: 'Type or generate', mono: true },
    { key: 'website', label: 'Website', placeholder: 'example.com' },
  ],
  secureNote: [],
  card: [
    { key: 'cardholder', label: 'Cardholder' },
    { key: 'number', label: 'Number', placeholder: '0000 0000 0000 0000', mono: true },
    { key: 'expiryMonth', label: 'Expiry month', placeholder: '1–12' },
    { key: 'expiryYear', label: 'Expiry year', placeholder: '2030' },
    { key: 'cvv', label: 'Security code', mono: true },
    { key: 'pin', label: 'PIN', mono: true },
    { key: 'billingAddress', label: 'Billing address' },
  ],
  identity: [
    { key: 'firstName', label: 'First name' },
    { key: 'lastName', label: 'Last name' },
    { key: 'dob', label: 'Date of birth', placeholder: 'YYYY-MM-DD' },
    { key: 'documentType', label: 'Document', placeholder: 'Passport' },
    { key: 'documentNumber', label: 'Document number', mono: true },
    { key: 'issuingCountry', label: 'Issuing country' },
    { key: 'expiry', label: 'Expires', placeholder: 'YYYY-MM-DD' },
    { key: 'address', label: 'Address' },
    { key: 'phone', label: 'Phone' },
    { key: 'email', label: 'Email' },
  ],
};

const EMPTY_FIELDS = Object.freeze({}) as Readonly<Partial<Record<FieldKey, string>>>;

export interface NewItemSheetProps {
  /** Vaults to choose between, for §14's chips. */
  vaults: readonly VaultSummaryDto[];
  /** Which vault is selected when the sheet opens. */
  defaultVaultId: string | undefined;
  /** Dismiss without saving. */
  onClose: () => void;
  /** Called with the saved item's title after a successful write. */
  onCreated: (title: string) => void;
  onFailed: (message: string) => void;
}

export function NewItemSheet({
  vaults,
  defaultVaultId,
  onClose,
  onCreated,
  onFailed,
}: NewItemSheetProps) {
  const [kind, setKind] = useState<ItemKindDto>('login');
  const [name, setName] = useState('');
  const [fields, setFields] = useState<Partial<Record<FieldKey, string>>>(EMPTY_FIELDS);
  const [notes, setNotes] = useState('');
  const [vaultId, setVaultId] = useState(defaultVaultId ?? '');
  const [strength, setStrength] = useState<StrengthDto | null>(null);
  const [saving, setSaving] = useState(false);
  const nameField = useRef<HTMLInputElement>(null);

  useEffect(() => {
    nameField.current?.focus();
  }, []);

  const set = useCallback((key: FieldKey, value: string) => {
    setFields((prev) => ({ ...prev, [key]: value }));
  }, []);

  const password = fields.password ?? '';

  useEffect(() => {
    // §14: "updates live as the password is typed". Scored in Rust so the meter agrees
    // with the security report's verdict rather than approximating it in TS.
    if (kind !== 'login') return;
    let live = true;
    passwordStrength(password, [name, fields.username ?? '', fields.website ?? '']).then(
      (result) => {
        if (live) setStrength(result);
      },
      () => {
        // A failed score must not claim a band. An empty meter with no label is the
        // honest rendering, and §7.4's "never 'safe'" is the same rule.
        if (live) setStrength(null);
      },
    );
    return () => {
      live = false;
    };
  }, [kind, password, name, fields.username, fields.website]);

  const initials = useMemo(() => monogram(name), [name]);
  const ready = name.trim().length > 0 && vaultId !== '';
  const vaultName = vaults.find((v) => v.id === vaultId)?.name ?? '';

  function generate() {
    generatorPassword({
      length: 20,
      lowercase: true,
      uppercase: true,
      digits: true,
      symbols: true,
      avoidAmbiguous: false,
    }).then(
      (generated) => {
        set('password', generated.value);
      },
      () => {
        onFailed('Could not generate a password');
      },
    );
  }

  function save(event: React.FormEvent) {
    event.preventDefault();
    if (!ready || saving) return;
    setSaving(true);
    itemUpsert({
      id: null,
      vaultId,
      title: name.trim(),
      notes,
      tags: [],
      favorite: false,
      customFields: [],
      body: bodyFor(kind, fields),
    }).then(
      () => {
        setSaving(false);
        onCreated(name.trim());
        onClose();
      },
      () => {
        setSaving(false);
        onFailed('Could not save the item');
      },
    );
  }

  return (
    <div
      className="veil veil--sheet"
      role="presentation"
      onClick={onClose}
      onKeyDown={(event) => {
        if (event.key === 'Escape') onClose();
      }}
    >
      <form
        className="sheet"
        aria-label="New item"
        onClick={(event) => {
          event.stopPropagation();
        }}
        onSubmit={save}
      >
        <header className="sheet__header">
          <span className="sheet__glyph" aria-hidden="true">
            <Glyph name="add" />
          </span>
          <span className="sheet__titles">
            <span className="sheet__title">New item</span>
            <span className="sheet__sub">
              Encrypted on this device before it is written to disk.
            </span>
          </span>
        </header>

        <div className="sheet__body">
          <Segmented options={KINDS} value={kind} onChange={setKind} label="Item type" />

          <div className="sheet__identity">
            <IdentityTile
              size="xl"
              title={name}
              // §14 shows a monogram built from the typed name and a "+" while it is
              // empty. The *tone* is Rust's to choose (it hashes the title), so the
              // preview uses a fixed one rather than reimplementing that hash in
              // TypeScript, where the two could silently drift apart.
              icon={{ kind: 'monogram', initials: initials === '' ? '+' : initials, tone: 1 }}
            />
            <span className="sheet__name-block">
              <label className="sheet__label" htmlFor="new-item-name">
                Item name
              </label>
              <input
                id="new-item-name"
                ref={nameField}
                className="sheet__name"
                // The webview's own autofill must never touch these fields. A password
                // manager whose new-item form gets populated by the host browser's saved
                // form data is filling the vault with someone else's idea of the truth,
                // and the value would look typed. Every input here opts out.
                autoComplete="off"
                autoCorrect="off"
                spellCheck={false}
                value={name}
                placeholder={NAME_PLACEHOLDER[kind]}
                onChange={(event) => {
                  setName(event.target.value);
                }}
              />
            </span>
          </div>

          {FIELDS[kind].length === 0 ? null : (
            <Group>
              {FIELDS[kind].map((spec) => (
                <GroupRow key={spec.key} height="option">
                  <label className="sheet__field-label" htmlFor={`new-item-${spec.key}`}>
                    {spec.label}
                  </label>
                  <input
                    id={`new-item-${spec.key}`}
                    className={spec.mono ? 'sheet__input sheet__input--mono' : 'sheet__input'}
                    autoComplete="off"
                    autoCorrect="off"
                    spellCheck={false}
                    // Not `type="password"`: §14 draws the value in the clear here. The
                    // user typed it and is checking it; masking it would hide a typo in
                    // the one field where a typo is unrecoverable.
                    value={fields[spec.key] ?? ''}
                    placeholder={spec.placeholder ?? ''}
                    onChange={(event) => {
                      set(spec.key, event.target.value);
                    }}
                  />
                  {spec.key === 'password' ? (
                    <button type="button" className="sheet__generate" onClick={generate}>
                      <Glyph name="generate" />
                      Generate
                    </button>
                  ) : null}
                </GroupRow>
              ))}

              {kind === 'login' ? (
                <GroupRow height="field">
                  <span className="sheet__field-label">Strength</span>
                  <StrengthMeter filled={strength?.band ?? 0} label={strength?.label ?? ''} />
                  <span className="meter-label" data-band={strength?.band ?? 0}>
                    {strength?.label ?? ''}
                  </span>
                </GroupRow>
              ) : null}
            </Group>
          )}

          <div className="sheet__vaults" role="radiogroup" aria-label="Vault">
            {vaults.map((vault) => (
              <button
                key={vault.id}
                type="button"
                role="radio"
                aria-checked={vault.id === vaultId}
                className="vault-chip"
                data-selected={vault.id === vaultId ? '' : undefined}
                onClick={() => {
                  setVaultId(vault.id);
                }}
              >
                <span className="vault-chip__swatch" data-accent={vault.colorToken} />
                {vault.name}
              </button>
            ))}
          </div>

          <label className="sheet__label" htmlFor="new-item-notes">
            Notes
          </label>
          <textarea
            id="new-item-notes"
            className="sheet__notes"
            rows={3}
            value={notes}
            placeholder="Recovery codes, security questions, anything else worth keeping."
            onChange={(event) => {
              setNotes(event.target.value);
            }}
          />
        </div>

        <footer className="sheet__footer">
          <span className="sheet__footer-note">
            <Glyph name="lock" />
            {ready ? `Saves to ${vaultName}` : 'Give the item a name to save it'}
          </span>
          <span className="detail-spacer" />
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" type="submit" disabled={!ready || saving}>
            {saving ? 'Saving…' : 'Save item'}
          </Button>
        </footer>
      </form>
    </div>
  );
}

/** Up to two initials from the typed name, matching what Rust builds for the tile. */
function monogram(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  return words
    .slice(0, 2)
    .map((word) => {
      // By code point, which is what Rust's `chars()` iterates — so the preview shows
      // the same monogram the stored item will get rather than a UTF-16 half of one.
      const first = word.codePointAt(0);
      return first === undefined ? '' : String.fromCodePoint(first);
    })
    .join('')
    .toUpperCase();
}

/** Build the discriminated body the command expects from the flat field state. */
function bodyFor(kind: ItemKindDto, f: Partial<Record<FieldKey, string>>): ItemBodyInput {
  switch (kind) {
    case 'login':
      return {
        kind: 'login',
        username: f.username ?? '',
        password: f.password ?? '',
        urls: (f.website ?? '').trim() === '' ? [] : [(f.website ?? '').trim()],
        totp: null,
      };
    case 'secureNote':
      return { kind: 'secureNote' };
    case 'card':
      return {
        kind: 'card',
        cardholder: f.cardholder ?? '',
        number: f.number ?? '',
        // A non-numeric or out-of-range entry becomes 0, which Rust rejects rather
        // than storing a month of 13. Parsing here and silently clamping would hide
        // the typo instead.
        expiryMonth: Number.parseInt(f.expiryMonth ?? '', 10) || 0,
        expiryYear: Number.parseInt(f.expiryYear ?? '', 10) || 0,
        cvv: f.cvv ?? '',
        pin: f.pin ?? '',
        billingAddress: f.billingAddress ?? '',
      };
    case 'identity':
      return {
        kind: 'identity',
        firstName: f.firstName ?? '',
        lastName: f.lastName ?? '',
        dob: f.dob ?? '',
        documentType: f.documentType ?? '',
        documentNumber: f.documentNumber ?? '',
        issuingCountry: f.issuingCountry ?? '',
        expiry: f.expiry ?? '',
        address: f.address ?? '',
        phone: f.phone ?? '',
        email: f.email ?? '',
      };
  }
}
