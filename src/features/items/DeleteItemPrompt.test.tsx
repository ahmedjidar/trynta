// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The two gates in front of deleting an item.
 *
 * The enforcement is in Rust — `item_delete` verifies the master password through the
 * store's own unlock path and compares the typed title itself, so a caller that skips
 * this dialog gets nowhere. What these test is that the dialog agrees with it: the
 * button must not enable on something Rust will refuse, and must not stay disabled on
 * something Rust would accept. A confirmation box that disagrees with the command
 * behind it is either a dead end or a false sense of security.
 */

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DeleteItemPrompt } from './DeleteItemPrompt';

const itemDelete = vi.hoisted(() => vi.fn());
vi.mock('../../ipc', () => ({ itemDelete }));

const TITLE = 'Northline Bank';

function open(onDeleted = vi.fn(), onCancel = vi.fn()) {
  render(<DeleteItemPrompt id="item-1" title={TITLE} onDeleted={onDeleted} onCancel={onCancel} />);
  return { onDeleted, onCancel };
}

/** The submit button, which is the gate under test. */
function deleteButton(): HTMLButtonElement {
  const node = screen.getByRole('button', { name: /^delete$/i });
  if (!(node instanceof HTMLButtonElement)) throw new Error('not a button');
  return node;
}

describe('DeleteItemPrompt', () => {
  beforeEach(() => {
    itemDelete.mockReset();
    itemDelete.mockResolvedValue(undefined);
  });

  it('refuses to submit until both the name and a password are given', async () => {
    const user = userEvent.setup();
    open();

    expect(deleteButton()).toBeDisabled();

    await user.type(screen.getByLabelText(`Type ${TITLE} to confirm`), TITLE);
    expect(deleteButton()).toBeDisabled();

    await user.type(screen.getByLabelText('Master password'), 'correct horse battery');
    expect(deleteButton()).toBeEnabled();
  });

  it('does not accept a name that is merely close', async () => {
    const user = userEvent.setup();
    open();

    await user.type(screen.getByLabelText('Master password'), 'correct horse battery');
    await user.type(screen.getByLabelText(`Type ${TITLE} to confirm`), 'Northline');

    expect(deleteButton()).toBeDisabled();
    expect(itemDelete).not.toHaveBeenCalled();
  });

  it('accepts the name in any case, matching what Rust compares', async () => {
    const user = userEvent.setup();
    open();

    await user.type(screen.getByLabelText('Master password'), 'correct horse battery');
    await user.type(screen.getByLabelText(`Type ${TITLE} to confirm`), 'northline bank');

    // Rust trims and compares case-insensitively. Disabling here on a value the
    // command would accept would be a dead end with no explanation.
    expect(deleteButton()).toBeEnabled();
  });

  it('sends the id, the password and the typed title to Rust', async () => {
    const user = userEvent.setup();
    const { onDeleted } = open();

    await user.type(screen.getByLabelText(`Type ${TITLE} to confirm`), TITLE);
    await user.type(screen.getByLabelText('Master password'), 'correct horse battery');
    await user.click(deleteButton());

    // All three, because all three are checked on the other side. A call that sent
    // only the id would be the frontend-only gate this deliberately is not.
    expect(itemDelete).toHaveBeenCalledWith('item-1', 'correct horse battery', TITLE);
    expect(onDeleted).toHaveBeenCalled();
  });

  it('keeps the item and drops the password when Rust refuses', async () => {
    itemDelete.mockRejectedValue(new Error('wrongPassword'));
    const user = userEvent.setup();
    const { onDeleted } = open();

    await user.type(screen.getByLabelText(`Type ${TITLE} to confirm`), TITLE);
    await user.type(screen.getByLabelText('Master password'), 'not the password');
    await user.click(deleteButton());

    expect(onDeleted).not.toHaveBeenCalled();
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    // The password box is cleared rather than left populated: a failed attempt should
    // not leave the master password sitting in a field on an unattended screen.
    expect(screen.getByLabelText('Master password')).toHaveValue('');
    expect(deleteButton()).toBeDisabled();
  });

  it('does not delete when the user backs out', async () => {
    const user = userEvent.setup();
    const { onCancel } = open();

    await user.click(screen.getByRole('button', { name: /keep it/i }));
    expect(onCancel).toHaveBeenCalled();
    expect(itemDelete).not.toHaveBeenCalled();
  });
});
