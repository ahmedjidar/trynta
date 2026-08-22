// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Item list behaviour — SPEC-V1 §7.1, and the accessibility gaps the design lists.
 *
 * These test the things the handoff called out as *not* implemented in the prototype,
 * because those are the ones a visual review will not catch:
 *
 * - rows are options in a listbox, not divs with onClick;
 * - arrow keys are bound to the list, not to `window` — the handoff's gap 2, whose
 *   symptom is that typing Down in the search field moves the selection;
 * - ⌘C / Ctrl+C copies the selected item's primary secret **without opening it**, and
 *   the copy goes to Rust rather than through any value the webview holds.
 */

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ItemList } from './ItemList';
import { useNavigation } from '../../app/navigation';
import type { ItemSummaryDto, RiskKindDto } from '../../ipc';

function item(id: string, title: string, extra: Partial<ItemSummaryDto> = {}): ItemSummaryDto {
  return {
    id,
    vaultId: 'v1',
    kind: 'login',
    title,
    subtitle: `${title.toLowerCase()}@example.test`,
    hasTotp: false,
    icon: { kind: 'shape', seed: 0x1234_5678, tone: 3 },
    isFavorite: false,
    isShared: false,
    revision: 1,
    updatedAt: 0,
    ...extra,
  };
}

const ITEMS = [item('a', 'Acme'), item('b', 'Bank'), item('c', 'Cloud')];

/**
 * The list asks the query layer for the `data:` URIs of any custom icons (ADD-001 tier 2).
 * Every row here resolves to a generated shape, so the hook is disabled and issues no
 * request — but the client still has to exist, the same as it does in the app. Rebuilt per
 * test in `beforeEach` so nothing caches across cases.
 */
let client = new QueryClient();

const wrap = {
  wrapper: ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  ),
};

function setup(overrides: Partial<Parameters<typeof ItemList>[0]> = {}) {
  const onCopy = vi.fn();
  const onNew = vi.fn();
  render(
    <ItemList
      items={ITEMS}
      risks={{}}
      vaultNames={{}}
      onCopy={onCopy}
      onNew={onNew}
      modifierKey="Ctrl"
      {...overrides}
    />,
    wrap,
  );
  return { onCopy, onNew };
}

describe('item list', () => {
  beforeEach(() => {
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    useNavigation.setState({
      selectedId: null,
      search: '',
      sort: 'recentlyUsed',
      filters: { weak: false, hasTotp: false, shared: false },
    });
  });

  it('exposes rows as options in a listbox', () => {
    setup();
    const listbox = screen.getByRole('listbox', { name: 'Items' });
    expect(listbox).toBeInTheDocument();
    expect(screen.getAllByRole('option')).toHaveLength(3);
  });

  it('makes the list one tab stop and the rows none', () => {
    setup();
    // The property that matters: a 10,000-item vault costs one tab press to reach and
    // one to leave. The rows are options addressed by `aria-activedescendant`, not
    // focusable elements — the header controls precede the list in tab order, which is
    // DOM order and correct.
    expect(screen.getByRole('listbox', { name: 'Items' })).toHaveAttribute('tabindex', '0');
    for (const option of screen.getAllByRole('option')) {
      expect(option).not.toHaveAttribute('tabindex');
    }
  });

  it('moves the selection with arrow keys and reports it to assistive tech', async () => {
    const user = userEvent.setup();
    setup();
    const listbox = screen.getByRole('listbox', { name: 'Items' });
    listbox.focus();
    expect(listbox).toHaveFocus();

    await user.keyboard('{ArrowDown}');
    expect(useNavigation.getState().selectedId).toBe('a');
    expect(listbox).toHaveAttribute('aria-activedescendant', 'item-a');

    await user.keyboard('{ArrowDown}');
    expect(useNavigation.getState().selectedId).toBe('b');

    await user.keyboard('{ArrowUp}');
    expect(useNavigation.getState().selectedId).toBe('a');
  });

  it('clamps at both ends instead of wrapping', async () => {
    const user = userEvent.setup();
    setup();
    screen.getByRole('listbox', { name: 'Items' }).focus();

    await user.keyboard('{End}');
    expect(useNavigation.getState().selectedId).toBe('c');
    await user.keyboard('{ArrowDown}');
    expect(useNavigation.getState().selectedId).toBe('c');

    await user.keyboard('{Home}');
    expect(useNavigation.getState().selectedId).toBe('a');
    await user.keyboard('{ArrowUp}');
    expect(useNavigation.getState().selectedId).toBe('a');
  });

  it('copies the selected item without opening it', async () => {
    const user = userEvent.setup();
    const { onCopy } = setup();
    screen.getByRole('listbox', { name: 'Items' }).focus();
    await user.keyboard('{ArrowDown}{ArrowDown}');

    await user.keyboard('{Control>}c{/Control}');
    expect(onCopy).toHaveBeenCalledExactlyOnceWith('b');
    // §7.1: "copies the primary secret of the selected item without opening it".
    // Selection is unchanged, and nothing navigated.
    expect(useNavigation.getState().selectedId).toBe('b');
  });

  it('does not copy when nothing is selected', async () => {
    const user = userEvent.setup();
    const { onCopy } = setup();
    const listbox = screen.getByRole('listbox', { name: 'Items' });
    listbox.focus();

    await user.keyboard('{Control>}c{/Control}');
    expect(onCopy).not.toHaveBeenCalled();
  });

  it('toggles quick filters as pressed buttons', async () => {
    const user = userEvent.setup();
    setup();
    const weak = screen.getByRole('button', { name: 'Weak' });
    expect(weak).toHaveAttribute('aria-pressed', 'false');

    await user.click(weak);
    expect(useNavigation.getState().filters.weak).toBe(true);
  });

  it('offers no shared filter, because sharing is V2', () => {
    // §7.5: "Never a toggle that does nothing." The filter exists on the wire and
    // must not exist in the UI until it does something.
    setup();
    expect(screen.queryByRole('button', { name: /shared/i })).toBeNull();
  });

  it('names the query in the empty state, and does not when there is none', () => {
    useNavigation.setState({ search: 'zzz' });
    const { unmount } = render(
      <ItemList
        items={[]}
        risks={{}}
        vaultNames={{}}
        onCopy={vi.fn()}
        onNew={vi.fn()}
        modifierKey="Ctrl"
      />,
      wrap,
    );
    expect(screen.getByText(/No items match/)).toBeInTheDocument();
    unmount();

    useNavigation.setState({ search: '' });
    render(
      <ItemList
        items={[]}
        risks={{}}
        vaultNames={{}}
        onCopy={vi.fn()}
        onNew={vi.fn()}
        modifierKey="Ctrl"
      />,
      wrap,
    );
    expect(screen.getByText(/Nothing here yet/)).toBeInTheDocument();
  });

  it('labels the risk dot rather than relying on colour alone', () => {
    render(
      <ItemList
        items={[item('a', 'Acme')]}
        risks={{ a: 'breached' }}
        vaultNames={{}}
        onCopy={vi.fn()}
        onNew={vi.fn()}
        modifierKey="Ctrl"
      />,
      wrap,
    );
    // The design conveys risk with a 6px coloured dot. Colour alone is not an
    // accessible signal, so it carries a name too.
    expect(screen.getByRole('img', { name: /found in a breach/i })).toBeInTheDocument();
  });

  it('names the finding the dot is actually for', () => {
    // Every kind used to reach the same two labels: breached, or "Weak password"
    // for everything else. An item whose only finding is that the service offers
    // two-factor would then tell a screen-reader user their password was weak,
    // which is a false statement about a security state — worse than silence.
    const cases: [RiskKindDto, RegExp][] = [
      ['breached', /found in a breach/i],
      ['weak', /^Weak password$/i],
      ['reused', /reused on another item/i],
      ['missingTwoFactor', /two-factor available, not set up/i],
    ];

    for (const [kind, name] of cases) {
      const view = render(
        <ItemList
          items={[item('a', 'Acme')]}
          risks={{ a: kind }}
          vaultNames={{}}
          onCopy={vi.fn()}
          onNew={vi.fn()}
          modifierKey="Ctrl"
        />,
        wrap,
      );
      expect(screen.getByRole('img', { name }), kind).toBeInTheDocument();
      // `weak` is the only kind that may say "weak".
      if (kind !== 'weak') {
        expect(screen.queryByRole('img', { name: /^Weak password$/i }), kind).toBeNull();
      }
      view.unmount();
    }
  });

  it('keeps the two colours the design drew, whatever the finding', () => {
    // `theme/dynamic.css` defines `.dot` in danger and warning and nothing else,
    // so a third tone would be inventing a colour. Amber means "needs attention";
    // the label says which kind of attention.
    for (const [kind, tone] of [
      ['breached', 'danger'],
      ['weak', 'warning'],
      ['reused', 'warning'],
      ['missingTwoFactor', 'warning'],
    ] as [RiskKindDto, string][]) {
      const view = render(
        <ItemList
          items={[item('a', 'Acme')]}
          risks={{ a: kind }}
          vaultNames={{}}
          onCopy={vi.fn()}
          onNew={vi.fn()}
          modifierKey="Ctrl"
        />,
        wrap,
      );
      expect(screen.getByRole('img'), kind).toHaveAttribute('data-tone', tone);
      view.unmount();
    }
  });

  it('marks the identity tile decorative when the title is already visible', () => {
    setup();
    // The tile sits next to the item name in every composition, so announcing
    // it would read the name twice.
    expect(screen.queryByRole('img', { name: 'Acme' })).toBeNull();
    expect(screen.getByText('Acme')).toBeInTheDocument();
  });
});
