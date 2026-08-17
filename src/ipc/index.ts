/**
 * The app's entire IPC surface.
 *
 * Features import from here and never from `@tauri-apps/api` — an eslint rule
 * enforces it (CLAUDE.md §5), so this barrel is the complete list of what the
 * frontend can ask Rust to do.
 *
 * Request and response types under `./generated/` are emitted from the Rust
 * definitions by `ts-rs` during `cargo test` and committed. Never hand-edit
 * them; change the Rust type and regenerate.
 */

export { IpcError, IpcTransportError } from './client';

export {
  accountCreate,
  accountExists,
  accountLock,
  accountReauth,
  accountState,
  accountStatus,
  accountUnlock,
  appPlatformInfo,
  generatorHistoryClear,
  generatorHistoryCopy,
  generatorHistoryList,
  generatorPassphrase,
  generatorPassword,
  generatorPin,
  itemActivity,
  itemCopyField,
  itemDelete,
  itemGet,
  itemRestore,
  itemRevealField,
  itemsList,
  itemToggleFavorite,
  itemUpsert,
  securityBreachCheck,
  securityReportRun,
  vaultAdd,
  vaultDelete,
  vaultRename,
  vaultSetColor,
  themeDelete,
  themeImport,
  themeList,
  themeSet,
  totpCurrent,
  updateCheck,
  updateChecksSetEnabled,
  updateInstall,
  vaultsList,
} from './commands';

export type { AccountStatus } from './generated/AccountStatus';
export type { ActivityEventDto } from './generated/ActivityEventDto';
export type { ActivityKindDto } from './generated/ActivityKindDto';
export type { AppError } from './generated/AppError';
export type { BreachCheckDto } from './generated/BreachCheckDto';
export type { CustomFieldDto } from './generated/CustomFieldDto';
export type { GeneratedDto } from './generated/GeneratedDto';
export type { GeneratedKindDto } from './generated/GeneratedKindDto';
export type { HistoryEntryDto } from './generated/HistoryEntryDto';
export type { PassphraseOptionsDto } from './generated/PassphraseOptionsDto';
export type { PasswordOptionsDto } from './generated/PasswordOptionsDto';
export type { TotpCodeDto } from './generated/TotpCodeDto';
export type { CustomFieldInput } from './generated/CustomFieldInput';
export type { CustomFieldKindDto } from './generated/CustomFieldKindDto';
export type { ItemBodyInput } from './generated/ItemBodyInput';
export type { ItemDetailDto } from './generated/ItemDetailDto';
export type { ItemDraftInput } from './generated/ItemDraftInput';
export type { ItemKindDto } from './generated/ItemKindDto';
export type { ItemSourceDto } from './generated/ItemSourceDto';
export type { ItemSummaryDto } from './generated/ItemSummaryDto';
export type { LabelledValue } from './generated/LabelledValue';
export type { ListQueryDto } from './generated/ListQueryDto';
export type { PlatformInfo } from './generated/PlatformInfo';
export type { QuickFiltersDto } from './generated/QuickFiltersDto';
export type { SecretFieldDto } from './generated/SecretFieldDto';
export type { HealthBreakdownDto } from './generated/HealthBreakdownDto';
export type { HealthTermDto } from './generated/HealthTermDto';
export type { ReuseGroupDto } from './generated/ReuseGroupDto';
export type { RiskDto } from './generated/RiskDto';
export type { RiskKindDto } from './generated/RiskKindDto';
export type { SecretPresence } from './generated/SecretPresence';
export type { SecurityReportDto } from './generated/SecurityReportDto';
export type { SortOrderDto } from './generated/SortOrderDto';
export type { ThemeCatalogDto } from './generated/ThemeCatalogDto';
export type { ThemeDto } from './generated/ThemeDto';
export type { ThemeModeDto } from './generated/ThemeModeDto';
export type { ThemeVariantDto } from './generated/ThemeVariantDto';
export type { TotpAlgorithmDto } from './generated/TotpAlgorithmDto';
export type { TotpConfigInput } from './generated/TotpConfigInput';
export type { UpdateCheckDto } from './generated/UpdateCheckDto';
export type { UpdateInfoDto } from './generated/UpdateInfoDto';
export type { UpdateStatusDto } from './generated/UpdateStatusDto';
export type { VaultKindDto } from './generated/VaultKindDto';
export type { VaultStateDto } from './generated/VaultStateDto';
export type { VaultSummaryDto } from './generated/VaultSummaryDto';
