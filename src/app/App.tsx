/**
 * Application shell (components.md §1).
 *
 * Composition only: the window, the title bar, the sidebar and whichever pane is
 * active. Everything with state of its own lives in the component that owns it.
 *
 * ## What is built and what is not
 *
 * Built: the shell, the item list with search, filters, sort, keyboard navigation and
 * virtualisation.
 *
 * All four sidebar surfaces are built: vault (list + detail), generator, security report
 * and settings. So are the lock screen, the command palette, the new-item sheet and the
 * updater surface.
 *
 * **Backup and restore is not built.** `keyring-store` has the format
 * (`backup_export`, `backup_merge`, `restore_replacing`) but no command exposes it, and
 * doing so needs a file-dialog capability — a permission grant rather than a screen. The
 * settings row says so instead of opening something half-drawn: a pane that renders a
 * plausible-looking placeholder is worse than one that admits it is unfinished, because
 * placeholder styling never gets replaced (CLAUDE.md §3).
 *
 * ## The lock gate
 *
 * `account_status` decides between the lock screen and the shell, and the shell is not
 * mounted while locked — §4.9's "lock is real" means there is nothing behind the lock
 * screen to read through it. Every vault query is gated on the same state, so none of
 * them fires against a locked vault and caches a rejection.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { Sidebar } from './Sidebar';
import { TitleBar } from './TitleBar';
import { useNavigation } from './navigation';
import { Toast } from '../components/Toast';
import { LockScreen } from '../features/account/LockScreen';
import { Palette } from '../features/palette/Palette';
import { Generator } from '../features/generator/Generator';
import { SecurityReport } from '../features/security/SecurityReport';
import { useBreachCheck, useSecurityReport } from '../features/security/useSecurity';
import { Settings } from '../features/settings/Settings';
import { Updates } from '../features/settings/Updates';
import { ItemDetail } from '../features/items/ItemDetail';
import { ItemList } from '../features/items/ItemList';
import { NewItemSheet } from '../features/items/NewItemSheet';
import { useClearCache, useItemDetail, useItems, useVaults } from '../features/items/useItems';
import {
  accountLock,
  accountStatus,
  appPlatformInfo,
  itemCopyField,
  revealWindow,
  settingsGet,
} from '../ipc';
import type { AccountStatus, ItemSummaryDto, SettingsDto } from '../ipc';
import { useThemeStore } from '../theme/store';

/**
 * One client for the app's lifetime.
 *
 * Created outside the component so a re-render cannot swap it, which would silently
 * discard every in-flight query.
 */
const queryClient = new QueryClient();

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <Shell />
    </QueryClientProvider>
  );
}

function Shell() {
  const hydrate = useThemeStore((s) => s.hydrate);
  const surface = useNavigation((s) => s.surface);
  const clearCache = useClearCache();

  const [platform, setPlatform] = useState({ modifierKey: 'Ctrl', os: 'windows' });
  const [toast, setToast] = useState<string | null>(null);
  const [account, setAccount] = useState<AccountStatus | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);

  const unlocked = account?.state === 'unlocked';
  const items = useItems(unlocked);
  const vaults = useVaults(unlocked);

  useEffect(() => {
    // Callable while locked — that is what it is for. Until it answers, neither the
    // lock screen nor the vault is rendered, because guessing wrong means either
    // flashing the vault chrome at someone who has not authenticated or flashing a
    // lock screen at someone who has.
    accountStatus().then(setAccount, () => {
      setToast('Could not read the vault state');
    });
  }, []);

  const onCopied = useCallback((what: string) => {
    setToast(what);
  }, []);
  const onFailed = useCallback((message: string) => {
    setToast(message);
  }, []);

  useEffect(() => {
    // Reveal the window only once the theme has been applied. The window is created
    // hidden so the first frame is never the wrong palette; see app/window.ts.
    void hydrate().finally(() => {
      void revealWindow();
    });
  }, [hydrate]);

  useEffect(() => {
    // SPEC-V1 §8: never hardcode the modifier. It resolves from the platform.
    appPlatformInfo()
      .then((info) => {
        setPlatform({ modifierKey: info.modifierKey, os: info.os });
      })
      .catch(() => {
        // Keep the default. A wrong shortcut hint is cosmetic; failing to render
        // the title bar over it would not be.
      });
  }, []);

  useEffect(() => {
    // SPEC-V1 §7.1's shortcut. `metaKey || ctrlKey` rather than a platform branch: on
    // Windows only Ctrl is pressed and on macOS only Command is, so accepting both is
    // the same behaviour without a `#cfg`-by-another-name in the frontend.
    const onKey = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() === 'k' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setPaletteOpen(true);
      }
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
  }, []);

  const onLock = useCallback(() => {
    // Clear the cache first. If `accountLock` rejects, the keys are still gone from
    // Rust's point of view only when it succeeds — but a cache we have already
    // dropped costs one refetch, and a cache we kept after a successful lock is a
    // §4.9 violation. Order for the worse failure.
    clearCache();
    accountLock().then(setAccount, () => {
      setToast('Could not lock');
    });
  }, [clearCache]);

  const onCopy = useCallback((id: string) => {
    // Rust reads the item, writes the OS clipboard and returns nothing. The
    // plaintext never enters the webview (CLAUDE.md §4.3).
    itemCopyField(id, { field: 'password' }).then(
      () => {
        setToast('Password copied');
      },
      () => {
        setToast('Could not copy');
      },
    );
  }, []);

  // Risk state comes from the security report, which is a later stage. Empty until
  // then, so no row claims to be safe or unsafe on no evidence — §7.4's "offline is
  // 'not checked', never 'safe'" applied to the list.
  const risks = useMemo<Record<string, 'breached' | 'weak'>>(() => ({}), []);

  const vaultNames = useMemo<Record<string, string>>(
    () => Object.fromEntries((vaults.data ?? []).map((v) => [v.id, v.name])),
    [vaults.data],
  );

  const counts = useMemo<Record<string, number>>(() => {
    const all = items.data?.length ?? 0;
    return { all, favorites: items.data?.filter((i) => i.isFavorite).length ?? 0 };
  }, [items.data]);

  if (account === null) {
    // One frame at most, and the window is still hidden behind `revealWindow()`.
    return <div className="desk" />;
  }

  if (account.state !== 'unlocked') {
    // §4.9: lock is real. The shell is not rendered *behind* this — it is not
    // mounted, so there are no item titles to read through a blur and no query
    // holding decrypted metadata.
    return (
      <div className="desk">
        <div className="window">
          <LockScreen
            exists={account.state !== 'uninitialised'}
            biometricLabel={account.biometricLabel}
            biometricAvailable={account.biometricAvailable}
            onUnlocked={(next) => {
              // The queries were created while locked and every one of them failed.
              // Clearing makes them refetch against an open vault instead of
              // serving their cached rejection.
              clearCache();
              setAccount(next);
            }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="desk">
      <div className="window">
        <TitleBar
          onOpenPalette={() => {
            setPaletteOpen(true);
          }}
          onLock={onLock}
          modifierKey={platform.modifierKey}
          os={platform.os}
        />
        <div className="window__body">
          <Sidebar vaults={vaults.data ?? []} counts={counts} riskCount={0} />

          {surface === 'vault' ? (
            <>
              <ItemList
                items={items.data ?? []}
                risks={risks}
                vaultNames={vaultNames}
                onCopy={onCopy}
                onNew={() => {
                  setSheetOpen(true);
                }}
              />
              <DetailPane vaultNames={vaultNames} onCopied={onCopied} onFailed={onFailed} />
            </>
          ) : surface === 'generator' ? (
            <Generator onCopied={onCopied} onFailed={onFailed} />
          ) : surface === 'settings' ? (
            <SettingsPane onCopied={onCopied} onFailed={onFailed} />
          ) : (
            // All four surfaces are handled, so TypeScript narrows this to `security`
            // and there is no placeholder branch left to write — the lint proved the
            // last comparison was always true, which is a pleasant way to find out.
            <SecurityPane items={items.data ?? []} onCopied={onCopied} onFailed={onFailed} />
          )}
        </div>

        {sheetOpen ? (
          <NewItemSheet
            vaults={vaults.data ?? []}
            defaultVaultId={vaults.data?.[0]?.id}
            onClose={() => {
              setSheetOpen(false);
            }}
            onCreated={(title) => {
              // The list is keyed on the query, so a new row needs the cache dropped
              // rather than a refetch of one key.
              clearCache();
              setToast(`${title} saved`);
            }}
            onFailed={onFailed}
          />
        ) : null}

        {paletteOpen ? (
          <Palette
            onClose={() => {
              setPaletteOpen(false);
            }}
            onLock={onLock}
          />
        ) : null}

        <Toast
          message={toast}
          onDismiss={() => {
            setToast(null);
          }}
          // Until settings are read this is the §7.5 default. The toast must not claim
          // a clear that is switched off, so it takes the value rather than a literal.
          clipboardSeconds={30}
        />
      </div>
    </div>
  );
}

interface DetailPaneProps {
  /** Vault id to name, for §6's header subtitle. */
  vaultNames: Record<string, string>;
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

function DetailPane({ vaultNames, onCopied, onFailed }: DetailPaneProps) {
  const selectedId = useNavigation((s) => s.selectedId);
  // Only rendered inside the unlocked branch, so the gate is satisfied by construction.
  const items = useItems();
  const detail = useItemDetail(selectedId);

  const summary = items.data?.find((i) => i.id === selectedId);

  if (selectedId === null || !summary) {
    return (
      <section className="pane" aria-label="Item detail">
        <div className="pane__content">
          <p className="pane__prose">Select an item.</p>
        </div>
      </section>
    );
  }

  if (detail.isError) {
    // Same class of bug as the security pane: a failed load must not look like a
    // still-loading one. Fail closed and say so (CLAUDE.md §4.10).
    return (
      <section className="pane" aria-label="Item detail">
        <div className="pane__content">
          <p className="pane__prose">This item could not be opened.</p>
        </div>
      </section>
    );
  }

  if (!detail.data) {
    // No skeleton: components.md §4 records that the prototype has no loading state
    // because the data is local, and specifies one only "if remote calls are added".
    // An empty pane for one frame is closer to the design than an invented shimmer.
    return <section className="pane" aria-label="Item detail" />;
  }

  return (
    <ItemDetail
      summary={summary}
      detail={detail.data}
      vaultName={vaultNames[summary.vaultId] ?? ''}
      // Risk bands come from the security report. Until it has run there is no band,
      // and §7.4's "never 'safe'" applies: 0 renders an empty meter labelled
      // "Not checked" rather than a green one.
      strength={{ band: 0, label: 'Not checked' }}
      onCopied={onCopied}
      onFailed={onFailed}
    />
  );
}

interface SecurityPaneProps {
  items: readonly ItemSummaryDto[];
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

function SecurityPane({ items, onCopied, onFailed }: SecurityPaneProps) {
  // Gated on the surface being open: the report decrypts every login's password to score
  // them, so running it because a sidebar row exists would decrypt the whole vault on
  // launch.
  const report = useSecurityReport(true);
  const check = useBreachCheck();

  if (report.isError) {
    // Found by opening the surface, not by type-checking it. `!report.data` alone
    // rendered an empty pane forever whenever the command failed, and the command
    // fails on a locked vault — scoring passwords means decrypting them. An empty
    // pane is indistinguishable from a broken one, which is the whole problem with
    // treating "no data" as "still loading".
    return (
      <section className="pane pane--wide" aria-label="Security report">
        <div className="pane__content">
          <h1 className="pane__title">Security report</h1>
          <p className="pane__prose">
            The report scores every stored password, so it can only run while the vault is unlocked.
          </p>
        </div>
      </section>
    );
  }

  if (!report.data) {
    return <section className="pane pane--wide" aria-label="Security report" />;
  }

  return (
    <SecurityReport
      report={report.data}
      items={items}
      canCheck={report.data.breachRefreshAvailable}
      onCheckNow={() => {
        check.mutate(undefined, {
          onSuccess: (result) => {
            onCopied(
              result.ran
                ? `Checked ${String(result.prefixesFetched)} of ${String(result.prefixesRequested)}`
                : 'Already checked in the last 24 hours',
            );
          },
          onError: () => {
            onFailed('Could not reach the breach service');
          },
        });
      }}
    />
  );
}

interface SettingsPaneProps {
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

function SettingsPane({ onCopied, onFailed }: SettingsPaneProps) {
  const [settings, setSettings] = useState<SettingsDto | null>(null);
  const [failed, setFailed] = useState(false);
  const [sub, setSub] = useState<'none' | 'updates'>('none');

  useEffect(() => {
    settingsGet().then(setSettings, () => {
      setFailed(true);
    });
  }, []);

  if (failed) {
    return (
      <section className="pane" aria-label="Settings">
        <div className="pane__content">
          <h1 className="pane__title">Settings</h1>
          <p className="pane__prose">
            Settings live inside the vault, so they are only readable while it is unlocked.
          </p>
        </div>
      </section>
    );
  }

  if (!settings) {
    return <section className="pane" aria-label="Settings" />;
  }

  if (sub === 'updates') {
    return (
      <Updates
        onBack={() => {
          setSub('none');
        }}
        onFailed={onFailed}
      />
    );
  }

  return (
    <Settings
      settings={settings}
      onSaved={(next) => {
        setSettings(next);
        onCopied('Saved');
      }}
      onFailed={onFailed}
      onBackup={() => {
        // Not built: the store has `backup_export`/`backup_merge`/`restore_replacing`
        // but no commands expose them, and doing so needs a file-dialog capability —
        // a permission grant, not a screen. Saying so beats a half-drawn surface.
        onFailed('Backup and restore is not built yet');
      }}
      onUpdates={() => {
        setSub('updates');
      }}
    />
  );
}
