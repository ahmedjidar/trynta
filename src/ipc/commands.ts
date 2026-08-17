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
import type { BreachCheckDto } from './generated/BreachCheckDto';
import type { GeneratedDto } from './generated/GeneratedDto';
import type { HistoryEntryDto } from './generated/HistoryEntryDto';
import type { PassphraseOptionsDto } from './generated/PassphraseOptionsDto';
import type { PasswordOptionsDto } from './generated/PasswordOptionsDto';
import type { TotpCodeDto } from './generated/TotpCodeDto';
import type { UpdateCheckDto } from './generated/UpdateCheckDto';
import type { ActivityEventDto } from './generated/ActivityEventDto';
import type { ItemDetailDto } from './generated/ItemDetailDto';
import type { ItemDraftInput } from './generated/ItemDraftInput';
import type { ItemSummaryDto } from './generated/ItemSummaryDto';
import type { ListQueryDto } from './generated/ListQueryDto';
import type { PlatformInfo } from './generated/PlatformInfo';
import type { SecretFieldDto } from './generated/SecretFieldDto';
import type { SecurityReportDto } from './generated/SecurityReportDto';
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

// ── TOTP (SPEC-V1 §7.2) ─────────────────────────────────────────────────────

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
