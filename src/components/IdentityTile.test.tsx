// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The identity tile never renders an empty square.
 *
 * The reported bug: removing a custom icon left a blank tile until something else
 * caused a refetch. The cause was not the removal — Rust drops the bytes and bumps the
 * revision immediately — but this component. A `custom` icon with no bytes rendered
 * nothing at all, and there is a window on every removal where exactly that is the
 * state: the item's row still says `custom` because the list has not refetched, while
 * the bytes are already gone.
 *
 * The same window opens on first paint of any custom item, before its fetch lands. So
 * this is not a race to be timed out of; it is a state the component has to draw.
 */

import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { IdentityTile } from './IdentityTile';
import type { IconDto } from '../ipc';

const CUSTOM: IconDto = { kind: 'custom' };
const BUNDLED: IconDto = { kind: 'bundled', key: 'github', themed: false, darkInk: true };
const SHAPE: IconDto = { kind: 'shape', seed: 7, tone: 3 };

/** The tile element itself, whichever branch drew it. */
function tile(): HTMLElement {
  const node = document.querySelector('.tile');
  if (!(node instanceof HTMLElement)) throw new Error('no tile rendered');
  return node;
}

describe('IdentityTile', () => {
  it('falls back to the default mark when a custom icon has no bytes yet', () => {
    render(<IdentityTile icon={CUSTOM} title="Northline" customSrc={undefined} />);

    // The failing behaviour was an element with no children at all.
    expect(tile().childElementCount).toBeGreaterThan(0);
    expect(document.querySelector('.fallback-mark')).toBeInTheDocument();
    expect(document.querySelector('img')).toBeNull();
  });

  it('draws the custom icon once its bytes arrive', () => {
    render(<IdentityTile icon={CUSTOM} title="Northline" customSrc="data:image/png;base64,AAAA" />);

    const img = document.querySelector('img');
    expect(img).toBeInTheDocument();
    expect(img).toHaveAttribute('src', 'data:image/png;base64,AAAA');
    expect(document.querySelector('.fallback-mark')).toBeNull();
  });

  it('draws a bundled mark from the app bundle, never a remote URL', () => {
    render(<IdentityTile icon={BUNDLED} title="GitHub" theme="dark" />);

    const img = document.querySelector('img');
    expect(img).toHaveAttribute('src', '/icons/github.svg');
    // ADD-001: nothing here may construct a request to anywhere else.
    expect(img?.getAttribute('src')).not.toMatch(/^https?:/);
  });

  it('chips a mark whose ink is too dark to read on the dark tile', () => {
    render(<IdentityTile icon={BUNDLED} title="GitHub" theme="dark" />);
    expect(tile()).toHaveAttribute('data-chip', 'light');
  });

  it('uses the fallback mark for an unmapped item', () => {
    render(<IdentityTile icon={SHAPE} title="Router" />);
    expect(document.querySelector('.fallback-mark')).toBeInTheDocument();
    expect(tile()).toHaveAttribute('data-tone', 'fallback');
  });
});
