/**
 * Application shell.
 *
 * Composition only: the window, the title bar, the sidebar and whichever pane is active.
 * Everything with state of its own lives in the component that owns it.
 *
 * ## There is no window
 *
 * The design is presented as a picture of a desktop — grey desk, rounded card, drop
 * shadow, three traffic lights. That is a presentation wrapper around the app, not part
 * of it. A real desktop build mounts the app directly, fills the OS window, draws no
 * background beyond `--surface-app`, and lets the OS draw the title-bar buttons. Nothing
 * below may recreate that chrome for a pane, a dialog, a sheet or a card.
 *
 * ## The lock gate
 *
 * `account_status` decides between the lock screen and the shell, and the shell is not
 * mounted while locked — §4.9's "lock is real" means there is nothing behind the lock
 * screen to read through it. Every vault query is gated on the same state, so none of
 * them fires against a locked vault and caches a rejection.
 *
 * ## Where the risk data comes from
 *
 * The item list's risk dots and the detail pane's strength band are the security
 * report's, read from cache rather than fetched: scoring decrypts every login's password,
 * so running it to colour a dot would decrypt the whole vault on launch. Until the user
 * opens the report there is no band, which is §7.4's *"offline is 'not checked', never
 * 'safe'"* applied to the surfaces that only borrow the result.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { Sidebar } from './Sidebar';
import { TitleBar } from './TitleBar';
import { WindowControls } from './WindowControls';
import { WindowFrame } from './WindowFrame';
import { useDragRegion } from './useDragRegion';
import { bindZoomShortcuts } from './zoom';
import { useNavigation } from './navigation';
import { Toast } from '../components/Toast';
import { LockScreen } from '../features/account/LockScreen';
import { Palette } from '../features/palette/Palette';
import { Generator } from '../features/generator/Generator';
import { SecurityReport } from '../features/security/SecurityReport';
import {
  useBreachCheck,
  useCachedSecurityReport,
  useSecurityReport,
} from '../features/security/useSecurity';
import { Backup } from '../features/settings/Backup';
import { Settings } from '../features/settings/Settings';
import { Updates } from '../features/settings/Updates';
import { ItemDetail } from '../features/items/ItemDetail';
import { ItemList } from '../features/items/ItemList';
import { NewItemSheet } from '../features/items/NewItemSheet';
import {
  useAllItems,
  useClearCache,
  useItemDetail,
  useItems,
  useVaults,
} from '../features/items/useItems';
import {
  accountLock,
  accountStatus,
  appPlatformInfo,
  itemCopyField,
  revealWindow,
  settingsGet,
} from '../ipc';
import type { AccountStatus, ItemSummaryDto, SecurityReportDto, SettingsDto } from '../ipc';
import { useThemeStore } from '../theme/store';

/**
 * One client for the app's lifetime.
 *
 * Created outside the component so a re-render cannot swap it, which would silently
 * discard every in-flight query.
 */
const queryClient = new QueryClient();

/** Band and label per risk kind, for the detail pane's strength row. */
/**
 * Risks that say something about the *password*, and the band each implies.
 *
 * `missingTwoFactor` is deliberately absent. It is a fact about the service, not
 * about the password, and a strong unique password on a site that offers 2FA is
 * still a strong unique password. Mapping it here would have labelled those items
 * "Weak" through the fallback below, which is a worse lie than saying nothing.
 */
const EMPTY_ITEMS: readonly ItemSummaryDto[] = Object.freeze([]);

const RISK_BANDS: Record<string, { band: number; label: string }> = {
  breached: { band: 1, label: 'Breached' },
  weak: { band: 1, label: 'Weak' },
  reused: { band: 2, label: 'Reused' },
};

/** The root of the application. */
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
  const [settings, setSettings] = useState<SettingsDto | null>(null);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const lockBarDrag = useDragRegion();

  const unlocked = account?.state === 'unlocked';
  const items = useItems(unlocked);
  const everything = useAllItems(unlocked);
  const vaults = useVaults(unlocked);
  const report = useCachedSecurityReport();

  useEffect(() => {
    // Callable while locked — that is what it is for. Until it answers, neither the
    // lock screen nor the vault is rendered, because guessing wrong means either
    // flashing the vault chrome at someone who has not authenticated or flashing a
    // lock screen at someone who has.
    accountStatus().then(setAccount, () => {
      setToast('Could not read the vault state');
    });
  }, []);

  useEffect(() => {
    // The toast promises a clipboard clear, so it has to know the configured interval
    // rather than repeat the design's literal "30s".
    if (!unlocked) return;
    settingsGet().then(setSettings, () => {
      // A settings read that fails is not worth a toast: the screen that needs them says
      // so itself, and the only thing lost here is the toast's suffix.
    });
  }, [unlocked]);

  const onCopied = useCallback((what: string) => {
    setToast(what);
  }, []);
  const onFailed = useCallback((message: string) => {
    setToast(message);
  }, []);

  useEffect(() => {
    // Reveal the window only once the theme has been applied. The window is created
    // hidden so the first frame is never the wrong palette; see ipc/window.ts.
    void hydrate().finally(() => {
      void revealWindow();
    });
  }, [hydrate]);

  useEffect(() => {
    // Ctrl/Cmd with +, - and 0. Bound before first paint so the starting level is
    // applied in the same frame the shell mounts, rather than as a visible resize.
    return bindZoomShortcuts();
  }, []);

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

  const onLock = useCallback(() => {
    // Clear the cache first. If `accountLock` rejects, the keys are still gone from
    // Rust's point of view only when it succeeds — but a cache we have already
    // dropped costs one refetch, and a cache we kept after a successful lock is a
    // §4.9 violation. Order for the worse failure.
    clearCache();
    setSettings(null);
    accountLock().then(setAccount, () => {
      setToast('Could not lock');
    });
  }, [clearCache]);

  useEffect(() => {
    // §7.1 and §7.9's two global shortcuts. `metaKey || ctrlKey` rather than a platform
    // branch: on Windows only Ctrl is pressed and on macOS only Command is, so accepting
    // both is the same behaviour without a `#cfg`-by-another-name in the frontend.
    const onKey = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      const key = event.key.toLowerCase();
      if (key === 'k') {
        event.preventDefault();
        setPaletteOpen(true);
      } else if (key === 'l') {
        event.preventDefault();
        onLock();
      }
    };
    globalThis.addEventListener('keydown', onKey);
    return () => {
      globalThis.removeEventListener('keydown', onKey);
    };
  }, [onLock]);

  const onVaultReplaced = useCallback(() => {
    // The vault file was rewritten and Rust locked the session. Everything cached
    // describes a vault that no longer exists, so drop it and re-read the state —
    // which puts the lock screen back up for the restored vault's own password.
    clearCache();
    setSettings(null);
    accountStatus().then(setAccount, () => {
      setToast('Could not read the vault state');
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

  const risks = useMemo<Record<string, 'breached' | 'weak'>>(() => {
    const map: Record<string, 'breached' | 'weak'> = {};
    for (const risk of report.data?.risks ?? []) {
      // Breached outranks weak outranks reused, and the list has one dot per row.
      if (risk.kind === 'breached') map[risk.itemId] = 'breached';
      else map[risk.itemId] ??= 'weak';
    }
    return map;
  }, [report.data]);

  const vaultNames = useMemo<Record<string, string>>(
    () => Object.fromEntries((vaults.data ?? []).map((v) => [v.id, v.name])),
    [vaults.data],
  );

  const counts = useMemo<Record<string, number>>(() => {
    // From the unfiltered query, not the current one: a count that changes when you
    // click a category is a count of the view rather than of the vault.
    const rows = everything.data ?? [];
    return {
      all: rows.length,
      favorites: rows.filter((i) => i.isFavorite).length,
      login: rows.filter((i) => i.kind === 'login').length,
      secureNote: rows.filter((i) => i.kind === 'secureNote').length,
      card: rows.filter((i) => i.kind === 'card').length,
      identity: rows.filter((i) => i.kind === 'identity').length,
    };
  }, [everything.data]);

  if (account === null) {
    // One frame at most, and the window is still hidden behind `revealWindow()`.
    return <WindowFrame />;
  }

  if (account.state !== 'unlocked') {
    // §4.9: lock is real. The shell is not rendered *behind* this — it is not
    // mounted, so there are no item titles to read through a blur and no query
    // holding decrypted metadata.
    //
    // The frame still wraps it: the window is the window whether or not the vault is
    // open, and a lock screen with square corners inside a rounded window is a seam.
    return (
      <WindowFrame>
        {/* A locked window is still a window: it has to be movable, minimisable and
            closable. Without this the only way out of a locked app is Task Manager. */}
        <div
          data-drag-region
          {...lockBarDrag}
          className="relative z-[8] flex h-[52px] shrink-0 items-center justify-end pr-3"
        >
          {platform.os === 'macos' ? null : <WindowControls />}
        </div>
        <LockScreen
          exists={account.state !== 'uninitialised'}
          onUnlocked={(next) => {
            // The queries were created while locked and every one of them failed.
            // Clearing makes them refetch against an open vault instead of serving
            // their cached rejection.
            clearCache();
            setAccount(next);
          }}
        />
      </WindowFrame>
    );
  }

  return (
    <WindowFrame>
      <TitleBar
        onOpenPalette={() => {
          setPaletteOpen(true);
        }}
        onLock={onLock}
        modifierKey={platform.modifierKey}
        os={platform.os}
      />
      <div className="flex min-h-0 flex-1">
        <Sidebar
          vaults={vaults.data ?? []}
          counts={counts}
          riskCount={report.data?.risks.length ?? 0}
        />

        {surface === 'vault' ? (
          <div className="flex min-w-0 flex-1">
            <ItemList
              items={items.data ?? []}
              risks={risks}
              vaultNames={vaultNames}
              onCopy={onCopy}
              onNew={() => {
                setSheetOpen(true);
              }}
              modifierKey={platform.modifierKey}
            />
            <DetailPane
              vaultNames={vaultNames}
              report={report.data ?? null}
              onCopied={onCopied}
              onFailed={onFailed}
            />
          </div>
        ) : surface === 'generator' ? (
          <Generator onCopied={onCopied} onFailed={onFailed} />
        ) : surface === 'settings' ? (
          <SettingsPane
            settings={settings}
            onSettings={setSettings}
            onCopied={onCopied}
            onFailed={onFailed}
            onVaultReplaced={onVaultReplaced}
          />
        ) : (
          // All four surfaces are handled, so TypeScript narrows this to `security`
          // and there is no placeholder branch left to write.
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
          modifierKey={platform.modifierKey}
        />
      ) : null}

      <Toast
        message={toast}
        onDismiss={() => {
          setToast(null);
        }}
        // The toast promises a clear, so it states the interval that is actually
        // configured, and says nothing at all when clearing is switched off.
        clipboardSeconds={
          settings === null ? null : settings.clearClipboard ? settings.clipboardSeconds : null
        }
      />
    </WindowFrame>
  );
}

/** An empty pane carrying one line, for the states the design has no composition for. */
function PaneNotice({ label, children }: { label: string; children?: React.ReactNode }) {
  return (
    <section
      className="bg-surface-panel animate-pane-in flex min-w-0 flex-1 items-center justify-center overflow-y-auto"
      aria-label={label}
    >
      <p className="text-caption text-text-muted max-w-[42ch] px-10 text-center text-pretty">
        {children}
      </p>
    </section>
  );
}

interface DetailPaneProps {
  /** Vault id to name, for §6's header subtitle. */
  vaultNames: Record<string, string>;
  /** The last report, for the strength band. `null` until one has been run. */
  report: SecurityReportDto | null;
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
}

function DetailPane({ vaultNames, report, onCopied, onFailed }: DetailPaneProps) {
  const selectedId = useNavigation((s) => s.selectedId);
  const clearCache = useClearCache();
  // Only rendered inside the unlocked branch, so the gate is satisfied by construction.
  const items = useItems();
  const detail = useItemDetail(selectedId);

  const summary = items.data?.find((i) => i.id === selectedId);

  const strength = useMemo(() => {
    if (report === null) return { band: 0, label: 'Not checked' };
    // Risks arrive most-severe first, so the first one that describes the password
    // is the one to show. A kind with no band — today, missing 2FA — is skipped
    // rather than defaulted, because defaulting is how it would have read 'Weak'.
    const risk = report.risks.find(
      (r) => r.itemId === selectedId && RISK_BANDS[r.kind] !== undefined,
    );
    const band = risk === undefined ? undefined : RISK_BANDS[risk.kind];
    if (band !== undefined) return band;
    // Scored and not flagged. §7.4 has no per-item score, so the strongest thing the
    // report supports saying is that nothing flagged it.
    return { band: 4, label: 'Strong' };
  }, [report, selectedId]);

  if (selectedId === null || !summary) {
    return <PaneNotice label="Item detail">Select an item to see its details.</PaneNotice>;
  }

  if (detail.isError) {
    // A failed load must not look like a still-loading one. Fail closed and say so
    // (CLAUDE.md §4.10).
    return <PaneNotice label="Item detail">This item could not be opened.</PaneNotice>;
  }

  if (!detail.data) {
    // No skeleton: components.md §4 records that the design has no loading state because
    // the data is local, and specifies one only "if remote calls are added". An empty
    // pane for one frame is closer to the design than an invented shimmer.
    return <section className="bg-surface-panel min-w-0 flex-1" aria-label="Item detail" />;
  }

  return (
    <ItemDetail
      summary={summary}
      detail={detail.data}
      items={items.data ?? EMPTY_ITEMS}
      vaultName={vaultNames[summary.vaultId] ?? ''}
      strength={strength}
      onCopied={onCopied}
      onFailed={onFailed}
      onEdited={clearCache}
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
    // `!report.data` alone rendered an empty pane forever whenever the command failed,
    // and the command fails on a locked vault — scoring passwords means decrypting them.
    // An empty pane is indistinguishable from a broken one, which is the whole problem
    // with treating "no data" as "still loading".
    return (
      <PaneNotice label="Security report">
        The report scores every stored password, so it can only run while the vault is unlocked.
      </PaneNotice>
    );
  }

  if (!report.data) {
    return <section className="bg-surface-panel min-w-0 flex-1" aria-label="Security report" />;
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
  /** Settings, or `null` while the read is in flight. */
  settings: SettingsDto | null;
  /** Hand a fresh copy back to the shell, so the toast's promise stays accurate. */
  onSettings: (next: SettingsDto) => void;
  onCopied: (what: string) => void;
  onFailed: (message: string) => void;
  /** A restore that rewrote the vault file locks the session; re-read the state. */
  onVaultReplaced: () => void;
}

function SettingsPane({
  settings,
  onSettings,
  onCopied,
  onFailed,
  onVaultReplaced,
}: SettingsPaneProps) {
  const [failed, setFailed] = useState(false);
  const [sub, setSub] = useState<'none' | 'updates' | 'backup'>('none');

  useEffect(() => {
    if (settings !== null) return;
    settingsGet().then(onSettings, () => {
      setFailed(true);
    });
  }, [settings, onSettings]);

  if (failed) {
    return (
      <PaneNotice label="Settings">
        Settings live inside the vault, so they are only readable while it is unlocked.
      </PaneNotice>
    );
  }

  if (!settings) {
    return <section className="bg-surface-panel min-w-0 flex-1" aria-label="Settings" />;
  }

  if (sub === 'backup') {
    return (
      <Backup
        onBack={() => {
          setSub('none');
        }}
        onDone={onCopied}
        onFailed={onFailed}
        onVaultReplaced={onVaultReplaced}
      />
    );
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
        onSettings(next);
        onCopied('Saved');
      }}
      onFailed={onFailed}
      onBackup={() => {
        setSub('backup');
      }}
      onUpdates={() => {
        setSub('updates');
      }}
    />
  );
}
