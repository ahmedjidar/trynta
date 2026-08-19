/**
 * Typed bindings for every Rust command (SPEC-V1 §6).
 *
 * One function per `#[tauri::command]`, named in camelCase after the Rust
 * `domain_verb`. Request and response types are generated from the Rust
 * definitions by `ts-rs`, so this file cannot drift from the backend without CI
 * noticing.
 *
 * ## The one function that returns a secret
 *
 * {@link itemRevealField} is the only binding here whose resolved value is
 * plaintext, and CLAUDE.md §4.4 places obligations on its *callers* that no
 * signature can enforce: do not persist it, do not put it in a store, do not
 * include it in a log or an error, and clear it on blur, navigation or lock.
 *
 * {@link itemCopyField} exists so that copying never needs any of that. It
 * resolves to `void` because the plaintext goes from Rust straight to the OS
 * clipboard and never enters the webview at all.
 */

import { call, callVoid } from './client';
import type { AccountStatus } from './generated/AccountStatus';
import type { BackupPreviewDto } from './generated/BackupPreviewDto';
import type { BackupSummaryDto } from './generated/BackupSummaryDto';
import type { BreachCheckDto } from './generated/BreachCheckDto';
import type { GeneratedDto } from './generated/GeneratedDto';
import type { StrengthDto } from './generated/StrengthDto';
import type { HistoryEntryDto } from './generated/HistoryEntryDto';
import type { IconUploadDto } from './generated/IconUploadDto';
import type { PassphraseOptionsDto } from './generated/PassphraseOptionsDto';
import type { PasswordOptionsDto } from './generated/PasswordOptionsDto';
import type { ThemeCatalogDto } from './generated/ThemeCatalogDto';
import type { ThemeDto } from './generated/ThemeDto';
import type { ThemeModeDto } from './generated/ThemeModeDto';
import type { TotpCodeDto } from './generated/TotpCodeDto';
import type { UpdateCheckDto } from './generated/UpdateCheckDto';
import type { ActivityEventDto } from './generated/ActivityEventDto';
import type { ItemDetailDto } from './generated/ItemDetailDto';
import type { ItemDraftInput } from './generated/ItemDraftInput';
import type { MetaEditsInput } from './generated/MetaEditsInput';
import type { ItemSummaryDto } from './generated/ItemSummaryDto';
import type { ListQueryDto } from './generated/ListQueryDto';
import type { PlatformInfo } from './generated/PlatformInfo';
import type { SecretFieldDto } from './generated/SecretFieldDto';
import type { SecurityReportDto } from './generated/SecurityReportDto';
import type { SettingsDto } from './generated/SettingsDto';
import type { SettingsPatch } from './generated/SettingsPatch';
import type { TotpConfigInput } from './generated/TotpConfigInput';
import type { VaultStateDto } from './generated/VaultStateDto';
import type { VaultSummaryDto } from './generated/VaultSummaryDto';

// ── Account (SPEC-V1 §5, §6) ────────────────────────────────────────────────

/**
 * Whether a vault file exists on this machine, for the first-run decision.
 *
 * @throws {IpcTransportError}
 *
 * @beta
 */
export function accountExists(): Promise<boolean> {
  return call<boolean>('account_exists');
}

/**
 * The lock state alone, for a cheap poll.
 *
 * @throws {IpcTransportError}
 *
 * @beta
 */
export function accountState(): Promise<VaultStateDto> {
  return call<VaultStateDto>('account_state');
}

/**
 * Lock state plus the counts and capabilities the shell needs.
 *
 * Safe to call while locked; counts are zero until the vault is open, because
 * counting items requires the keys.
 *
 * @throws {IpcTransportError}
 *
 * @beta
 */
export function accountStatus(): Promise<AccountStatus> {
  return call<AccountStatus>('account_status');
}

/**
 * Create the vault, calibrate the KDF for this machine, and leave it unlocked.
 *
 * Slow by design — it measures Argon2 against a 700 ms target (SPEC-V1 §3.2) —
 * so callers should show progress rather than assume it returns promptly.
 *
 * @param masterPassword - The master password, as typed.
 * @throws {IpcError} `invalidState` if a vault already exists.
 *
 * @beta
 */
export function accountCreate(masterPassword: string): Promise<AccountStatus> {
  return call<AccountStatus>('account_create', { masterPassword });
}

/**
 * Unlock with the master password.
 *
 * @param masterPassword - The master password, as typed.
 * @throws {IpcError} `wrongPassword`, `backoff` with a retry delay,
 * `tamperDetected` if the file has been modified, or `noVault`.
 *
 * @example
 * ```ts
 * try {
 *   await accountUnlock(typed);
 * } catch (e) {
 *   if (e instanceof IpcError && e.error.kind === 'backoff') {
 *     showWait(e.error.retryInSeconds);
 *   }
 * }
 * ```
 *
 * @beta
 */
export function accountUnlock(masterPassword: string): Promise<AccountStatus> {
  return call<AccountStatus>('account_unlock', { masterPassword });
}

/**
 * Re-authenticate an unlocked vault after the reveal rate limit trips.
 *
 * Call this when {@link itemRevealField} throws `reauthRequired`, then retry the
 * reveal (SPEC-V1 §6).
 *
 * @param masterPassword - The master password, as typed.
 * @throws {IpcError} `wrongPassword`, or `locked` if the vault is not open.
 *
 * @beta
 */
export function accountReauth(masterPassword: string): Promise<void> {
  return callVoid('account_reauth', { masterPassword });
}

/**
 * Export an encrypted backup under its own passphrase (SPEC-V1 §7.8).
 *
 * Opens a save dialog; Rust writes the file. Resolves to `null` when the user
 * cancels, which is not an error.
 *
 * @param passphrase - The backup's own passphrase, not the master password. At
 * least 12 characters.
 * @returns What was written, or `null` on cancel.
 * @throws {IpcError} `locked`, `invalid` for a short passphrase, `storage`.
 *
 * @beta
 */
export function backupExport(passphrase: string): Promise<BackupSummaryDto | null> {
  return call<BackupSummaryDto | null>('backup_export', { passphrase });
}

/**
 * Open a backup and report what restoring it would do, without doing it.
 *
 * Opening authenticates the passphrase, the header MAC and the manifest signature,
 * so a preview that resolves describes something trustworthy. Nothing is written.
 *
 * @param passphrase - The backup's passphrase.
 * @returns The preview and the container's path, or `null` on cancel.
 * @throws {IpcError} `wrongPassword`, `tamperDetected`, `storage`.
 *
 * @beta
 */
export function backupPreview(passphrase: string): Promise<BackupPreviewDto | null> {
  return call<BackupPreviewDto | null>('backup_preview', { passphrase });
}

/**
 * Apply a restore (SPEC-V1 §7.8). Never partially applies.
 *
 * @param path - From a previous {@link backupPreview}. Re-opened and re-verified
 * rather than trusted.
 * @param passphrase - The backup's passphrase.
 * @param allowReplace - Required when the mode is `replace`, which destroys a vault
 * belonging to a different account.
 * @throws {IpcError} `wrongPassword`, `tamperDetected`, `invalid` when a replace was
 * not authorised, `storage`.
 *
 * @beta
 */
export function backupRestore(
  path: string,
  passphrase: string,
  allowReplace: boolean,
): Promise<BackupPreviewDto> {
  return call<BackupPreviewDto>('backup_restore', { path, passphrase, allowReplace });
}

/**
 * Lock the vault: wipe keys, drop the index, clear our clipboard entry.
 *
 * @throws {IpcTransportError}
 *
 * @beta
 */
export function accountLock(): Promise<AccountStatus> {
  return call<AccountStatus>('account_lock');
}

// ── Vaults (SPEC-V1 §4.2, §6) ───────────────────────────────────────────────

/**
 * Every live vault with its item count.
 *
 * @throws {IpcError} `locked`.
 *
 * @beta
 */
export function vaultsList(): Promise<VaultSummaryDto[]> {
  return call<VaultSummaryDto[]>('vaults_list');
}

/**
 * Create a vault.
 *
 * @param colorToken - A token *name* such as `vault.accent.3`. Never a colour
 * value: Rust rejects anything that is not a dotted alphanumeric token.
 * @returns The new vault's id.
 * @throws {IpcError} `invalid` on a bad name or token, `locked`.
 *
 * @beta
 */
export function vaultAdd(name: string, colorToken: string): Promise<string> {
  return call<string>('vault_add', { name, colorToken });
}

/**
 * Rename a vault.
 *
 * @throws {IpcError} `invalid` on an empty name, `notFound`, `locked`.
 *
 * @beta
 */
export function vaultRename(id: string, name: string): Promise<void> {
  return callVoid('vault_rename', { id, name });
}

/**
 * Change a vault's colour token.
 *
 * @param colorToken - A token name, as for {@link vaultAdd}.
 * @throws {IpcError} `invalid`, `notFound`, `locked`.
 *
 * @beta
 */
export function vaultSetColor(id: string, colorToken: string): Promise<void> {
  return callVoid('vault_set_color', { id, colorToken });
}

/**
 * Delete a vault.
 *
 * @param moveItemsTo - Vault to move this one's items into. `null` soft-deletes
 * them alongside it, recoverable for 30 days.
 * @throws {IpcError} `notFound`, `lastVaultRemaining` if it is the only vault,
 * `locked`.
 *
 * @beta
 */
export function vaultDelete(id: string, moveItemsTo: string | null): Promise<void> {
  return callVoid('vault_delete', { id, moveItemsTo });
}

// ── Items (SPEC-V1 §6, §7.1, §7.2) ──────────────────────────────────────────

/**
 * The item list: filtered, searched, sorted. Metadata only.
 *
 * Runs against the Rust-side index built from `meta_ct`, so no secret is
 * decrypted to produce it and none can appear in the result.
 *
 * @throws {IpcError} `locked`.
 *
 * @example
 * ```ts
 * const rows = await itemsList({
 *   source: { source: 'all' },
 *   filters: { weak: false, hasTotp: false, shared: false },
 *   sort: 'recentlyUpdated',
 *   search: '',
 * });
 * ```
 *
 * @beta
 */
export function itemsList(query: ListQueryDto): Promise<ItemSummaryDto[]> {
  return call<ItemSummaryDto[]>('items_list', { query });
}

/**
 * One item's detail: metadata, plus which secrets exist without their values.
 *
 * @throws {IpcError} `locked`, `notFound`.
 *
 * @beta
 */
export function itemGet(id: string): Promise<ItemDetailDto> {
  return call<ItemDetailDto>('item_get', { id });
}

/**
 * Reveal one secret field — the only plaintext path out of Rust.
 *
 * The resolved string must not be persisted, cached, put in a store, or included
 * in any log or error, and must be cleared on blur, navigation or lock
 * (CLAUDE.md §4.4). Hold it in component-local state and nowhere else.
 *
 * @throws {IpcError} `reauthRequired` once 20 reveals have happened in any
 * rolling 60 seconds — call {@link accountReauth} and retry. Also `locked`,
 * `notFound`, `noSuchField`.
 *
 * @example
 * ```ts
 * const password = await itemRevealField(id, { field: 'password' });
 * ```
 *
 * @beta
 */
export function itemRevealField(id: string, field: SecretFieldDto): Promise<string> {
  return call<string>('item_reveal_field', { id, field });
}

/**
 * Copy one secret field to the clipboard, entirely inside Rust.
 *
 * Resolves to `void` because the plaintext never enters the webview
 * (CLAUDE.md §4.3). Prefer this over {@link itemRevealField} wherever the user
 * only needs to paste the value.
 *
 * @throws {IpcError} `locked`, `notFound`, `noSuchField`, `clipboard`.
 *
 * @beta
 */
export function itemCopyField(id: string, field: SecretFieldDto): Promise<void> {
  return callVoid('item_copy_field', { id, field });
}

/**
 * Create an item, or update one when `draft.id` is set.
 *
 * @returns The item's id.
 * @throws {IpcError} `invalid` on an empty title, `locked`, `notFound` if the
 * vault or item is gone.
 *
 * @beta
 */
export function itemUpsert(draft: ItemDraftInput): Promise<string> {
  return call<string>('item_upsert', { draft });
}

/**
 * Soft-delete an item. Recoverable with {@link itemRestore} for 30 days.
 *
 * @throws {IpcError} `locked`, `notFound`.
 *
 * @beta
 */
export function itemDelete(id: string): Promise<void> {
  return callVoid('item_delete', { id });
}

/**
 * Restore a soft-deleted item.
 *
 * @throws {IpcError} `locked`, `notFound`.
 *
 * @beta
 */
export function itemRestore(id: string): Promise<void> {
  return callVoid('item_restore', { id });
}

/**
 * Apply non-secret edits to an item — the detail pane's edit mode.
 *
 * Distinct from {@link itemUpsert}: upsert rebuilds the secret envelope from the
 * draft, so routing a title change through it would mean holding the password in
 * the form and an empty field would wipe the stored one. This carries the sealed
 * secret across in Rust, so the form never sees it.
 *
 * @param id - Item to edit.
 * @param edits - Only the present fields are written.
 * @returns Whether anything changed. A no-op edit does not burn a revision.
 * @throws {IpcError} `invalid` for a blank title, `notFound`, `locked`.
 *
 * @example
 * ```ts
 * await itemEditMeta(id, { title: 'Renamed', username: 'ada@example.com' });
 * ```
 *
 * @beta
 */
export function itemEditMeta(id: string, edits: MetaEditsInput): Promise<boolean> {
  return call<boolean>('item_edit_meta', { id, edits });
}

/**
 * The user's own icon for an item, as a `data:` URI, or `null` if it has none.
 *
 * Only worth calling when the item's {@link IconDto} is `custom`; every other kind
 * renders without a round trip.
 *
 * The URI is safe to put in an `<img src>`: the production CSP is `img-src 'self'
 * data:`, and an SVG inside an `<img>` cannot execute script even if it contained any —
 * which it cannot, because Rust sanitised it on the way in.
 *
 * @throws {IpcError} `locked`, `notFound`, `storage`.
 *
 * @beta
 */
export function itemIcon(id: string): Promise<string | null> {
  return call<string | null>('item_icon', { id });
}

/**
 * Ask the user for an image file and attach it to an item.
 *
 * Opens a file dialog; **Rust reads, decodes, resizes and re-encodes it**. The webview
 * never receives the chosen file or its path, which is the point: an image found on the
 * internet is untrusted input, and a decoder is a parser.
 *
 * Resolves to `null` when the dialog is cancelled, which is not an error.
 *
 * @returns The processed size in bytes, so the UI can show what it cost.
 * @throws {IpcError} `invalid` if the image is refused — over 2 MB, not one of
 * SVG/PNG/JPEG/WebP/ICO, or an SVG carrying script or an external reference. Also
 * `locked`, `notFound`, `storage`.
 *
 * @beta
 */
export function itemSetIcon(id: string): Promise<IconUploadDto | null> {
  return call<IconUploadDto | null>('item_set_icon', { id });
}

/**
 * Remove an item's icon, so it falls back to the bundled mark or a generated one.
 *
 * @throws {IpcError} `locked`, `notFound`, `storage`.
 *
 * @beta
 */
export function itemClearIcon(id: string): Promise<void> {
  return callVoid('item_clear_icon', { id });
}

/**
 * Flip an item's favourite flag.
 *
 * @returns The flag's new value.
 * @throws {IpcError} `locked`, `notFound`.
 *
 * @beta
 */
export function itemToggleFavorite(id: string): Promise<boolean> {
  return call<boolean>('item_toggle_favorite', { id });
}

/**
 * Recent activity for one item, newest first.
 *
 * Kinds and timestamps only — never which field was involved.
 *
 * @param limit - Clamped to 100 by Rust.
 * @throws {IpcError} `locked`, `notFound`.
 *
 * @beta
 */
export function itemActivity(id: string, limit: number): Promise<ActivityEventDto[]> {
  return call<ActivityEventDto[]>('item_activity', { id, limit });
}

// ── Application (SPEC-V1 §8) ────────────────────────────────────────────────

/**
 * Platform facts, including the modifier-key label for keyboard hints.
 *
 * The source of `Cmd` versus `Ctrl`. SPEC-V1 §8 forbids a literal `⌘` anywhere
 * in source, so every shortcut hint resolves through this.
 *
 * @throws {IpcTransportError}
 *
 * @beta
 */
export function appPlatformInfo(): Promise<PlatformInfo> {
  return call<PlatformInfo>('app_platform_info');
}

// ── Generator (SPEC-V1 §7.3) ────────────────────────────────────────────────

/**
 * Generate a random password.
 *
 * The resolved value is plaintext — the second and last such path after
 * {@link itemRevealField}, because showing the user a password they can use is the
 * whole feature. Treat it the same way: component-local state, cleared on
 * navigation, never in a store and never in a log.
 *
 * @throws {IpcError} `locked`, or `crypto` if the OS randomness source failed.
 *
 * @example
 * ```ts
 * const { value, entropyBits } = await generatorPassword({
 *   length: 20, uppercase: true, lowercase: true,
 *   digits: true, symbols: true, avoidAmbiguous: false,
 * });
 * ```
 *
 * @beta
 */
export function generatorPassword(options: PasswordOptionsDto): Promise<GeneratedDto> {
  return call<GeneratedDto>('generator_password', { options });
}

/**
 * Generate a passphrase from the bundled EFF long wordlist.
 *
 * `separator` and `capitalise` add **zero** bits — the attacker knows the scheme —
 * and `entropyBits` ignores them. Never present them as strengthening anything.
 *
 * @throws {IpcError} `featureUnavailable` while the wordlist is not vendored,
 * `locked`, or `crypto`.
 *
 * @beta
 */
export function generatorPassphrase(options: PassphraseOptionsDto): Promise<GeneratedDto> {
  return call<GeneratedDto>('generator_passphrase', { options });
}

/**
 * Generate a numeric PIN.
 *
 * @param length - Clamped to 3–12 by Rust.
 * @throws {IpcError} `locked`, or `crypto`.
 *
 * @beta
 */
export function generatorPin(length: number): Promise<GeneratedDto> {
  return call<GeneratedDto>('generator_pin', { length });
}

/**
 * Score a password the user is typing, for §14's live strength row.
 *
 * Needs no unlocked vault — scoring is arithmetic over the string. The response
 * carries a band, a label and a crack estimate, never the password.
 *
 * @param password - The typed value. Already exposed to the webview by the form it
 * came from; nothing here stores it.
 * @param context - The item's own non-secret fields (title, username, website), which
 * lower the estimate for a password built out of them.
 *
 * @example
 * ```ts
 * const { band, label } = await passwordStrength(typed, [title, username]);
 * ```
 *
 * @beta
 */
export function passwordStrength(password: string, context: string[]): Promise<StrengthDto> {
  return call<StrengthDto>('password_strength', { password, context });
}

/**
 * The retained generator history, newest first, **without values**.
 *
 * Entries carry a kind, an entropy figure and a timestamp. The values stay in
 * Rust: use {@link generatorHistoryCopy} to put one on the clipboard.
 *
 * @throws {IpcError} `locked`.
 *
 * @beta
 */
export function generatorHistoryList(): Promise<HistoryEntryDto[]> {
  return call<HistoryEntryDto[]>('generator_history_list');
}

/**
 * Copy one history entry to the clipboard, entirely inside Rust.
 *
 * @throws {IpcError} `notFound` if the entry has expired or been cleared,
 * `locked`, `clipboard`.
 *
 * @beta
 */
export function generatorHistoryCopy(id: string): Promise<void> {
  return callVoid('generator_history_copy', { id });
}

/**
 * Forget the whole generator history.
 *
 * @throws {IpcError} `locked`.
 *
 * @beta
 */
export function generatorHistoryClear(): Promise<void> {
  return callVoid('generator_history_clear');
}

// ── Biometric unlock (SPEC-V1 §5, §7.5) ─────────────────────────────────────

/**
 * Whether biometric unlock is available on this device *and* set up for this vault.
 *
 * Both halves, because either one alone means the button cannot work: no hardware is
 * a fact about the machine, no enrolment is a fact about this vault. Safe to call
 * while locked — it is what the lock screen asks before offering the option.
 *
 * @throws {IpcError} `storage`.
 *
 * @beta
 */
export function biometricReady(): Promise<boolean> {
  return call<boolean>('biometric_ready');
}

/**
 * Turn biometric unlock on.
 *
 * Takes the master password because there is no moment when the app is holding it
 * and could enrol silently — it is used once at unlock and dropped. Rust verifies it
 * opens the vault before storing anything: enrolling an unverified string would
 * produce a biometric unlock that fails forever on a biometric that works fine.
 *
 * @param masterPassword - The master password, as typed.
 * @throws {IpcError} `wrongPassword`, `biometric` if the platform refuses, `storage`.
 *
 * @beta
 */
export function biometricEnable(masterPassword: string): Promise<void> {
  return callVoid('biometric_enable', { masterPassword });
}

/**
 * Turn biometric unlock off and destroy the stored secret.
 *
 * @throws {IpcError} `biometric` if the platform refuses to revoke, `storage`.
 *
 * @beta
 */
export function biometricDisable(): Promise<void> {
  return callVoid('biometric_disable');
}

/**
 * Unlock with the platform biometric.
 *
 * Raises the platform prompt — Windows Hello here. Failure is one error for every
 * cause: cancelled, no match, and enrolment invalidated all mean *use your password*,
 * and distinguishing them would tell an attacker which attempt got furthest.
 *
 * @throws {IpcError} `biometric` for any failure of the prompt, `invalidState` when
 * §5.1's fourteen-day master-password unlock is due, `wrongPassword` if the stored
 * secret no longer opens the vault.
 *
 * @beta
 */
export function accountUnlockBiometric(): Promise<AccountStatus> {
  return call<AccountStatus>('account_unlock_biometric');
}

// ── TOTP (SPEC-V1 §7.2) ─────────────────────────────────────────────────────

/**
 * Read a one-time-code setup the user pasted, as a URI or a bare secret.
 *
 * Accepts both things services hand out and calls "the code": the
 * `otpauth://totp/...` URI behind a QR image, and the bare base32 string sites
 * print when they offer no QR at all. Which one it is is detected, not asked.
 *
 * Parsing is in Rust so the parameters that reach storage are the ones the URI
 * carried. Dropping `algorithm=SHA256` in a TypeScript parser would store SHA-1
 * and generate codes that never work — a bug ADD-004 §④ records having shipped
 * once already.
 *
 * The returned object is ready to hand straight to {@link itemUpsert} as
 * `body.totp`.
 *
 * @param input - An `otpauth://` URI or a base32 secret, as pasted.
 * @throws {IpcError} `totpRejected`, carrying a {@link TotpRejectionDto} saying
 * which rule failed. The input is never echoed back in the error: it is a shared
 * secret.
 *
 * @beta
 */
export function totpParse(input: string): Promise<TotpConfigInput> {
  return call<TotpConfigInput>('totp_parse', { input });
}

/**
 * Attach, replace or remove an item's one-time-code setup.
 *
 * Pass `null` to remove one; nothing else about the item changes. Separate from
 * {@link itemEditMeta} because the seed belongs in `secret_ct`, and separate from
 * {@link itemUpsert} because the detail view does not hold the password.
 *
 * @returns Whether anything changed. Writing the same configuration twice does not
 * burn a revision.
 * @throws {IpcError} `notFound` if the item is absent or is not a login. Also
 * `locked`, `storage`.
 *
 * @beta
 */
export function itemSetTotp(id: string, totp: TotpConfigInput | null): Promise<boolean> {
  return call<boolean>('item_set_totp', { id, totp });
}

/**
 * The current one-time code for an item, with its countdown.
 *
 * Returns a code, never the seed. Poll it once per second to drive a countdown,
 * or once per `period` and derive the remainder locally.
 *
 * @throws {IpcError} `notFound` if the item has no TOTP configuration — which
 * includes a seed stored without its parameters, where a guessed SHA-1/6/30 would
 * produce a plausible code that never works. Also `locked`.
 *
 * @beta
 */
export function totpCurrent(id: string): Promise<TotpCodeDto> {
  return call<TotpCodeDto>('totp_current', { id });
}

/**
 * Put an item's current one-time code on the clipboard.
 *
 * The *code*, not the seed. Copying the seed and calling it the code — which is what
 * `itemCopyField(id, { field: 'totpSecret' })` does — puts a base32 string on the
 * clipboard that fails every verification prompt it is pasted into. The seed stays
 * reachable through the ordinary reveal path, which is where someone moving to
 * another authenticator would look for it.
 *
 * Written by Rust with the platform's secrecy markers and the same auto-clear as any
 * other copy; the plaintext never enters the webview.
 *
 * @throws {IpcError} `notFound` if the item has no configuration. Also `locked`,
 * `clipboard`.
 *
 * @beta
 */
export function totpCopyCurrent(id: string): Promise<void> {
  return callVoid('totp_copy_current', { id });
}

// ── Security report (SPEC-V1 §7.4) ──────────────────────────────────────────

/**
 * Run the security report.
 *
 * Makes no network requests. Anything the breach cache has no answer for comes
 * back in `notChecked` rather than counted as clean, and the UI must present it
 * that way — SPEC-V1 §7.4: *"Offline → 'not checked,' never 'safe.'"*
 *
 * `breakdown` is non-null exactly when `score` is. Render it: §7.4 requires the
 * arithmetic to be visible, not just the total.
 *
 * `twoFactorCapable` is `0` until the bundled 2FA directory ships, which is why
 * the breakdown's `twoFactor.weight` is `0` and the other three weights are
 * 43.75 / 31.25 / 25. Read the weights from the response rather than hardcoding
 * them, so shipping the directory does not need a frontend change.
 *
 * @throws {IpcError} `locked`.
 *
 * @beta
 */
export function securityReportRun(): Promise<SecurityReportDto> {
  return call<SecurityReportDto>('security_report_run');
}

/**
 * Refresh the HIBP range cache.
 *
 * Call this once after unlock. It is the only binding here that reaches the
 * network, and it enforces §7.4's cadence itself: inside 24 hours of the last
 * successful check it resolves with `ran: false` and sends nothing. There is
 * deliberately no way to force it.
 *
 * It may take tens of seconds — one request per distinct password — so do not
 * await it on a path the user is waiting behind. `prefixesFailed` above zero means
 * some items are now "not checked"; that is a state to show, not an error.
 *
 * @throws {IpcError} `locked`, `storage`.
 *
 * @beta
 */
export function securityBreachCheck(): Promise<BreachCheckDto> {
  return call<BreachCheckDto>('security_breach_check');
}

// ── Updates (SPEC-V1 §7.7) ──────────────────────────────────────────────────

/**
 * Ask whether a newer build exists.
 *
 * Works with the vault locked, and enforces §7.7's once-per-24-hours cadence
 * itself — call it on launch and let it decide. Read `status` rather than testing
 * `available` for null: `checkedRecently`, `checkFailed` and `disabled` all have
 * a null `available` and none of them means "you are up to date".
 *
 * `available.notes` is text supplied by the update endpoint. Render it as text.
 *
 * @throws {IpcError} `featureUnavailable` until a release endpoint and signing
 * public key are configured, `storage`.
 *
 * @beta
 */
export function updateCheck(): Promise<UpdateCheckDto> {
  return call<UpdateCheckDto>('update_check');
}

/**
 * Download, verify and install the pending update.
 *
 * Re-checks the manifest and its signature at the moment of install rather than
 * trusting an earlier {@link updateCheck}, so nothing stale can be applied. Does
 * not require the vault to be unlocked.
 *
 * Listen for `update://progress` — `[downloaded, total]` in bytes, `total` null
 * when the endpoint sends no `Content-Length` — to show real progress. On success
 * the app restarts to run the new binary, so this promise may never resolve.
 *
 * @throws {IpcError} `featureUnavailable` if no endpoint is configured,
 * `notFound` if nothing newer is being offered, `updateFailed` for a download,
 * signature or install failure — one discriminant for all three, deliberately.
 *
 * @beta
 */
export function updateInstall(): Promise<void> {
  return callVoid('update_install');
}

/**
 * Turn unattended update checks on or off.
 *
 * The other half of §7.7's "user-visible, disableable". Persists to app state, so
 * the preference is honoured on the next launch before the vault is unlocked. Read
 * the current position from {@link updateCheck}'s `checksEnabled`.
 *
 * Turning checks off does not disable {@link updateInstall}: the cadence governs
 * unattended checks, and a user who clicks "install" has asked for one.
 *
 * @throws {IpcError} `noVault` if no vault exists yet — the setting has nowhere to
 * live until then, `storage`.
 *
 * @beta
 */
export function updateChecksSetEnabled(enabled: boolean): Promise<void> {
  return callVoid('update_checks_set_enabled', { enabled });
}

// ── Theme (SPEC-V1 §7.6) ────────────────────────────────────────────────────

/**
 * Every theme the user can pick, plus the stored selection.
 *
 * Works with the vault locked, which is the point — the lock screen renders in the
 * user's mode. While locked, `imported` is empty and `locked` is `true`: theme
 * values live in the encrypted settings blob, so they are genuinely unavailable
 * rather than absent. Show the picker disabled, not empty.
 *
 * @throws {IpcError} `noVault` before a vault exists, `storage`.
 *
 * @beta
 */
export function themeList(): Promise<ThemeCatalogDto> {
  return call<ThemeCatalogDto>('theme_list');
}

/**
 * Set the mode, and optionally the active imported theme.
 *
 * Pass `id: null` for the built-in palette. An `id` naming no stored theme is
 * refused rather than stored, so the selection and what renders cannot disagree.
 *
 * @throws {IpcError} `notFound` for an unknown id, `locked` if an id is given while
 * locked — verifying it needs the settings blob — `storage`.
 *
 * @beta
 */
export function themeSet(id: string | null, mode: ThemeModeDto): Promise<void> {
  return callVoid('theme_set', { id, mode });
}

/**
 * Validate and store a theme document.
 *
 * Validation happens in Rust (SPEC-V1 §7.6) and this is the only way a theme enters
 * the app. Importing does **not** activate — call {@link themeSet} for that — so
 * adding a theme never changes what the user is looking at as a side effect.
 *
 * @throws {IpcError} `invalid` if the document is refused, which includes every
 * spelling of `url()` and a full theme list; `locked`; `storage`.
 *
 * @beta
 */
export function themeImport(document: string): Promise<ThemeDto> {
  return call<ThemeDto>('theme_import', { document });
}
/**
 * Choose a theme file and import it.
 *
 * The picker, the read and the validation all happen in Rust: the webview holds no
 * filesystem permission, which is why this exists alongside {@link themeImport}
 * rather than the frontend reading a file and passing the text.
 *
 * Importing does not activate — call {@link themeSet} for that.
 *
 * Resolves to `null` when the dialog is cancelled, which is not an error.
 *
 * @throws {IpcError} `invalid` if the document is refused, which includes every
 * spelling of `url()`; `locked`; `storage`.
 *
 * @beta
 */
export function themeImportFile(): Promise<ThemeDto | null> {
  return call<ThemeDto | null>('theme_import_file');
}

/**
 * Save a theme document to a file the user picks.
 *
 * The caller builds the document from the tokens resolved on `:root`; this writes it.
 * Rust owns the dialog and the write because the webview holds no filesystem
 * permission — the same reasoning as the icon upload and the theme import.
 *
 * @param document - The JSON to write.
 * @returns Whether a file was written. `false` means the dialog was cancelled, which
 * is not an error.
 * @throws {IpcError} `storage` if the file cannot be written.
 *
 * @beta
 */
export function themeExportFile(document: string): Promise<boolean> {
  return call<boolean>('theme_export_file', { document });
}

/**
 * Remove an imported theme, clearing the selection if it was active.
 *
 * @throws {IpcError} `notFound`, `locked`, `storage`.
 *
 * @beta
 */
export function themeDelete(id: string): Promise<void> {
  return callVoid('theme_delete', { id });
}

// ── Settings (SPEC-V1 §7.5) ─────────────────────────────────────────────────

/**
 * Everything the settings screen shows.
 *
 * Spans both stores — the encrypted blob and §4.5's plaintext carve-out — because that is
 * what a settings screen is. Requires an unlocked vault.
 *
 * `autofillAvailable` is always `false` in V1. Render §7.5's honest "not available yet"
 * state from it rather than a switch: "never a toggle that does nothing".
 *
 * @throws {IpcError} `locked`, `storage`.
 *
 * @beta
 */
export function settingsGet(): Promise<SettingsDto> {
  return call<SettingsDto>('settings_get');
}

/**
 * Apply a patch. Absent fields are left alone.
 *
 * Returns the settings as they are **after** the write, not what was asked for — the two
 * differ whenever a value is clamped, and rendering the request would show the user a
 * number that is not stored.
 *
 * @throws {IpcError} `biometric` if biometric unlock is switched on where no biometric
 * exists, `locked`, `storage`.
 *
 * @beta
 */
export function settingsSet(patch: SettingsPatch): Promise<SettingsDto> {
  return call<SettingsDto>('settings_set', { patch });
}
