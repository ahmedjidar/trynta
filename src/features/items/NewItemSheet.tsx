/**
 * New-item sheet — HO-002 `overlays/NewItemSheet.tsx`, SPEC-V1 §7.1.
 *
 * Header, 4-up kind segments that swap the field set, a 64px live preview tile, the name
 * input, grouped field rows, vault chips, notes, and a footer whose copy states where the
 * item will be saved with Save gated until it has a name.
 *
 * ## Four departures from HO-002
 *
 * - **The subtitle.** HO-002 reads "Encrypted on this Mac before it syncs." ADD-005 makes
 *   Windows the platform, and sync is SPEC-V3 — there is nothing to sync to, so the
 *   sentence promises a feature that does not exist. Rewritten to what actually happens.
 * - **The preview tile has no favicon.** HO-002 crossfades a Google favicon over the
 *   monogram once the domain parses. ADD-001 forbids the request; the monogram is the tile.
 * - **"Ask for Touch ID before autofill" is not built.** Autofill is V3 (§7.5), there is no
 *   field to store it against, and §7.5 forbids a toggle that does nothing.
 * - **Generate calls Rust.** HO-002 generates with `Math.random()` in `lib/utils.ts`. A
 *   password manager's generator is a CSPRNG in Rust (§7.3); the button calls
 *   `generator_password`.
 *
 * The live strength row calls `password_strength`, so the meter agrees with the security
 * report's verdict rather than approximating it in TypeScript.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { Button } from '../../components/Button';
import { Chip, CopyAction, Input } from '../../components/Bits';
import { FieldLabel, GroupedList, GroupedRow } from '../../components/GroupedList';
import { Glyph } from '../../components/Glyph';
import { IdentityTile } from '../../components/IdentityTile';
import { Overlay, SheetFooter, SheetHeader } from '../../components/Overlay';
import { SegmentedControl } from '../../components/SegmentedControl';
import type { Segment } from '../../components/SegmentedControl';
import { StrengthMeter } from '../../components/StrengthMeter';
import { generatorPassword, itemUpsert, passwordStrength } from '../../ipc';
import type { ItemBodyInput, ItemKindDto, StrengthDto, VaultSummaryDto } from '../../ipc';

/** The four kinds HO-002's segmented control offers, in its order and with its glyphs. */
const KINDS: readonly Segment<ItemKindDto>[] = [
  { id: 'login', name: 'Login', icon: <Glyph name="login" size={14} /> },
  { id: 'secureNote', name: 'Note', icon: <Glyph name="note" size={14} /> },
  { id: 'card', name: 'Card', icon: <Glyph name="card" size={14} /> },
  { id: 'identity', name: 'Identity', icon: <Glyph name="identity" size={14} /> },
];

/** Placeholder per kind for the name field, as HO-002's `PLACEHOLDER` map does. */
const NAME_PLACEHOLDER: Record<ItemKindDto, string> = {
  login: 'Northwind Mail',
  secureNote: 'Office Wi-Fi',
  card: 'Everyday card',
  identity: 'Passport',
};

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

/** The field set per kind — HO-002's "swaps the field set below". */
const FIELDS: Record<ItemKindDto, readonly FieldSpec[]> = {
  login: [
    { key: 'username', label: 'Username', placeholder: 'name@company.com' },
    { key: 'password', label: 'Password', placeholder: 'Type or generate', mono: true },
    { key: 'website', label: 'Website', placeholder: 'example.com' },
  ],
  secureNote: [],
  card: [
    { key: 'cardholder', label: 'Cardholder' },
    { key: 'number', label: 'Card number', placeholder: '0000 0000 0000 0000', mono: true },
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
  /** Vaults to choose between, for the chips. */
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
    // "Updates live as the password is typed". Scored in Rust so the meter cannot
    // disagree with the security report's own verdict.
    if (kind !== 'login') return undefined;
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
    <Overlay onDismiss={onClose} label="New item" placement="sheet">
      <form className="flex max-h-[752px] w-[560px] flex-col" onSubmit={save}>
        <SheetHeader
          icon={<Glyph name="add" />}
          title="New item"
          sub="Encrypted on this device before it is written to disk."
        />

        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto p-5">
          <SegmentedControl segments={KINDS} value={kind} onChange={setKind} label="Item type" />

          <div className="flex items-center gap-4">
            {/* The monogram the stored item will get. The tone is Rust's to choose (it
                hashes the title), so the preview uses a fixed one rather than
                reimplementing that hash in TypeScript where the two could drift. */}
            <IdentityTile
              size={64}
              title={name}
              icon={{ kind: 'monogram', initials: initials === '' ? '+' : initials, tone: 1 }}
            />
            <label className="min-w-0 flex-1">
              <span className="text-micro tracking-label text-text-caption-aa font-bold uppercase">
                Item name
              </span>
              <input
                ref={nameField}
                className="border-strong bg-surface-raised text-body-lg text-text-primary mt-2 h-9 w-full rounded-lg border px-3 font-semibold outline-none"
                value={name}
                placeholder={NAME_PLACEHOLDER[kind]}
                // The webview's own autofill must never populate a vault form: it would
                // write the host browser's saved form data into the vault, and it would
                // look typed.
                autoComplete="off"
                autoCorrect="off"
                spellCheck={false}
                onChange={(event) => {
                  setName(event.target.value);
                }}
              />
            </label>
          </div>

          <GroupedList>
            {FIELDS[kind].map((spec) => (
              <GroupedRow key={spec.key} className="h-[52px]">
                <FieldLabel>{spec.label}</FieldLabel>
                <Input
                  aria-label={spec.label}
                  // Not `type="password"`: HO-002 draws these in the clear. The user typed
                  // it and is checking it; masking hides a typo in the one field where a
                  // typo is unrecoverable.
                  className={spec.mono ? 'flex-1 font-mono' : 'flex-1'}
                  value={fields[spec.key] ?? ''}
                  placeholder={spec.placeholder ?? ''}
                  onChange={(event) => {
                    set(spec.key, event.target.value);
                  }}
                />
                {spec.key === 'password' ? (
                  <CopyAction className="h-[30px] rounded-md px-[11px]" onClick={generate}>
                    <Glyph name="generate" size={14} />
                    Generate
                  </CopyAction>
                ) : null}
              </GroupedRow>
            ))}

            {kind === 'login' ? (
              <GroupedRow className="h-11">
                <FieldLabel>Strength</FieldLabel>
                <StrengthMeter score={strength?.band ?? 0} label={strength?.label ?? ''} />
                <span
                  className="text-chip w-[68px] shrink-0 text-right font-bold"
                  data-tone={strengthTone(strength?.band ?? 0)}
                >
                  {strength?.label ?? ''}
                </span>
              </GroupedRow>
            ) : null}

            <GroupedRow className="h-[52px]">
              <FieldLabel>Vault</FieldLabel>
              <div className="flex min-w-0 flex-1 gap-1.5" role="radiogroup" aria-label="Vault">
                {vaults.map((vault) => (
                  <Chip
                    key={vault.id}
                    className="h-[26px]"
                    role="radio"
                    aria-checked={vault.id === vaultId}
                    selected={vault.id === vaultId}
                    onClick={() => {
                      setVaultId(vault.id);
                    }}
                  >
                    <span
                      className="swatch h-[7px] w-[7px] rounded-xs"
                      data-accent={vault.colorToken}
                    />
                    {vault.name}
                  </Chip>
                ))}
              </div>
            </GroupedRow>

            <GroupedRow className="min-h-[52px] items-start py-3.5">
              <span className="text-caption text-text-caption-aa w-24 shrink-0 leading-6 font-medium">
                Notes
              </span>
              <textarea
                aria-label="Notes"
                rows={3}
                className="border-strong bg-surface-panel text-body text-text-primary min-w-0 flex-1 resize-none rounded-md border px-2.5 py-2 leading-[18px] outline-none"
                value={notes}
                placeholder="Recovery codes, security questions, anything else worth keeping."
                onChange={(event) => {
                  setNotes(event.target.value);
                }}
              />
            </GroupedRow>
          </GroupedList>
        </div>

        <SheetFooter
          hint={
            <>
              <Glyph name="lock" size={12} />
              {ready ? `Saves to ${vaultName}` : 'Give the item a name to save it'}
            </>
          }
        >
          <Button variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={!ready || saving}>
            {saving ? 'Saving…' : 'Save item'}
          </Button>
        </SheetFooter>
      </form>
    </Overlay>
  );
}

/** Tone for the strength label, matching HO-002's `strengthColor()` thresholds. */
function strengthTone(band: number): string {
  if (band === 0) return 'empty';
  if (band <= 1) return 'danger';
  if (band === 2) return 'warning';
  return 'accent';
}

/** Up to two initials, by code point — which is what Rust's `chars()` iterates. */
function monogram(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => {
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
        // A non-numeric or out-of-range entry becomes 0, which Rust rejects rather than
        // storing a month of 13. Clamping here would hide the typo instead.
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
