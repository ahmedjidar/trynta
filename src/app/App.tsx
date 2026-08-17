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
 * Not built yet, and marked `UNSTYLED: awaiting <surface>` rather than approximated —
 * item detail, generator, security report, settings, command palette, and the
 * backup/restore and updater surfaces. HO-001 specifies all of them; they are the
 * remaining run-3 stages. A pane that renders a plausible-looking placeholder is worse
 * than one that says it is unfinished, because placeholder styling never gets replaced
 * (CLAUDE.md §3).
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { Sidebar } from './Sidebar';
import { TitleBar } from './TitleBar';
import { useNavigation } from './navigation';
import { ItemList } from '../features/items/ItemList';
import { useClearCache, useItems, useVaults } from '../features/items/useItems';
import { accountLock, appPlatformInfo, itemCopyField } from '../ipc';
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

  const items = useItems();
  const vaults = useVaults();
  const [modifierKey, setModifierKey] = useState('Ctrl');

  useEffect(() => {
    void hydrate();
  }, [hydrate]);

  useEffect(() => {
    // SPEC-V1 §8: never hardcode the modifier. It resolves from the platform.
    appPlatformInfo()
      .then((info) => {
        setModifierKey(info.modifierKey);
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
    void accountLock().catch(() => {
      // UNSTYLED: awaiting handoff toast. components.md §15 specifies the surface
      // that would report this.
    });
  }, [clearCache]);

  const onCopy = useCallback((id: string) => {
    // Rust reads the item, writes the OS clipboard and returns nothing. The
    // plaintext never enters the webview (CLAUDE.md §4.3).
    void itemCopyField(id, { field: 'password' }).catch(() => {
      // UNSTYLED: awaiting handoff toast.
    });
  }, []);

  // Risk state comes from the security report, which is a later stage. Empty until
  // then, so no row claims to be safe or unsafe on no evidence — §7.4's "offline is
  // 'not checked', never 'safe'" applied to the list.
  const risks = useMemo<Record<string, 'breached' | 'weak'>>(() => ({}), []);

  const counts = useMemo<Record<string, number>>(() => {
    const all = items.data?.length ?? 0;
    return { all, favorites: items.data?.filter((i) => i.isFavorite).length ?? 0 };
  }, [items.data]);

  return (
    <div className="desk">
      <div className="window">
        <TitleBar
          onOpenPalette={() => {
            // UNSTYLED: awaiting handoff command-palette (components.md §14).
          }}
          onLock={onLock}
          modifierKey={modifierKey}
        />
        <div className="window__body">
          <Sidebar vaults={vaults.data ?? []} counts={counts} riskCount={0} />

          {surface === 'vault' ? (
            <>
              <ItemList
                items={items.data ?? []}
                risks={risks}
                onCopy={onCopy}
                onNew={() => {
                  // UNSTYLED: awaiting handoff new-item-sheet (components.md §14).
                }}
              />
              <ItemDetailPlaceholder />
            </>
          ) : (
            <PanePlaceholder surface={surface} />
          )}
        </div>
      </div>
    </div>
  );
}

function ItemDetailPlaceholder() {
  const selectedId = useNavigation((s) => s.selectedId);
  return (
    <section className="pane pane--unstyled" aria-label="Item detail">
      <div className="pane__content">
        {/* UNSTYLED: awaiting handoff item-detail (components.md §6). */}
        <h2 className="pane__title">Item detail</h2>
        <p className="pane__prose">
          {selectedId === null
            ? 'Select an item.'
            : 'Not built yet — the detail pane is the next run-3 stage. HO-001 §6 specifies it.'}
        </p>
      </div>
    </section>
  );
}

function PanePlaceholder({ surface }: { surface: string }) {
  return (
    <section className="pane pane--unstyled" aria-label={surface}>
      <div className="pane__content">
        {/* UNSTYLED: awaiting handoff for this surface. HO-001 covers all of them;
            they are the remaining run-3 stages. */}
        <h2 className="pane__title">{surface}</h2>
        <p className="pane__prose">Not built yet.</p>
      </div>
    </section>
  );
}
