/**
 * The single place `invoke()` appears (CLAUDE.md §5).
 *
 * Everything else in the app calls a typed function from `./commands`, which
 * calls {@link call} here. An eslint rule enforces the boundary, and the point of
 * it is not tidiness: one file means one place to audit what crosses IPC, and one
 * place to mock in tests.
 *
 * ## Errors carry no arguments
 *
 * {@link call} deliberately never puts its `args` into the error it throws.
 * Arguments to `itemUpsert` contain the password the user just typed, and an
 * error message is the one string in a program guaranteed to end up in a log, a
 * toast, or a screenshot (CLAUDE.md §4.6). The command name is safe; the payload
 * is not.
 */

import { invoke } from '@tauri-apps/api/core';

import type { AppError } from './generated/AppError';

/**
 * An error returned by a Rust command, with its typed discriminant attached.
 *
 * The `message` is derived from the discriminant alone. Callers that need to
 * branch should read {@link IpcError.error}, not parse the message.
 *
 * @beta
 */
export class IpcError extends Error {
  /**
   * The typed error the Rust side returned.
   */
  readonly error: AppError;

  /**
   * The command that failed. Never its arguments.
   */
  readonly command: string;

  /**
   * @param command - Name of the failed command.
   * @param error - The typed error from Rust.
   */
  constructor(command: string, error: AppError) {
    super(`${command} failed: ${error.kind}`);
    this.name = 'IpcError';
    this.error = error;
    this.command = command;
  }
}

/**
 * An error whose shape we did not recognise — a panic, a missing command, or a
 * transport failure.
 *
 * Distinct from {@link IpcError} because the two need different handling: a
 * typed `AppError` is a normal outcome the UI has a message for, while this one
 * means something is wrong with the app itself.
 *
 * @beta
 */
export class IpcTransportError extends Error {
  /**
   * The command that failed.
   */
  readonly command: string;

  /**
   * @param command - Name of the failed command.
   */
  constructor(command: string) {
    // No detail, on purpose: an unrecognised rejection may be a Rust panic
    // message, and a panic message is exactly the string CLAUDE.md §4.6 says
    // must never be assumed secret-free.
    super(`${command} failed: the command did not return a recognised error`);
    this.name = 'IpcTransportError';
    this.command = command;
  }
}

/**
 * Narrow an unknown rejection to a typed {@link AppError}.
 *
 * Structural rather than exhaustive: `AppError` is `#[non_exhaustive]` in Rust,
 * so a build that learns a new variant before this file does should still get a
 * typed error rather than a transport failure.
 */
function isAppError(value: unknown): value is AppError {
  if (typeof value !== 'object' || value === null || !('kind' in value)) {
    return false;
  }
  const { kind } = value;
  return typeof kind === 'string';
}

/**
 * Invoke a Rust command and translate its rejection into a typed error.
 *
 * @param command - The `domain_verb` command name registered in `lib.rs`.
 * @param args - Command arguments, camelCase, matching the generated types.
 * @returns Whatever the command returns, as declared by the caller's `T`.
 * @throws {IpcError} When Rust returned a typed {@link AppError}.
 * @throws {IpcTransportError} When the rejection was not a recognisable error.
 *
 * @example
 * ```ts
 * const status = await call<AccountStatus>('account_status');
 * ```
 *
 * @beta
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (rejection: unknown) {
    if (isAppError(rejection)) {
      throw new IpcError(command, rejection);
    }
    throw new IpcTransportError(command);
  }
}

/**
 * {@link call} for a command that returns nothing.
 *
 * A Rust `()` serialises to JSON `null`, so the value is typed and discarded
 * here rather than at every call site.
 *
 * @param command - The command name.
 * @param args - Command arguments.
 * @throws {IpcError} As {@link call}.
 * @throws {IpcTransportError} As {@link call}.
 *
 * @beta
 */
export async function callVoid(command: string, args?: Record<string, unknown>): Promise<void> {
  await call<null>(command, args);
}
