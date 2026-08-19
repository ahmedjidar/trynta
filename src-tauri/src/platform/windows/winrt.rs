// SPDX-License-Identifier: AGPL-3.0-or-later
//! Blocking waits on `WinRT` async operations.
//!
//! The `windows` crate re-exports the four `WinRT` async types but keeps the
//! trait that awaits them (`windows_future::Async`, with `join`) private, and
//! its `Future` impls need an async runtime we do not otherwise want in the
//! platform layer. Both `KeyCredentialManager` entry points we call are async
//! and both are already blocking from the user's point of view — they put a
//! Hello prompt on screen and wait for a finger.
//!
//! So: set the completion handler, block on a channel, read the result. Twelve
//! lines, no async runtime, and no reliance on a private API that could move
//! again. `windows-future` is named directly because the async *types* are
//! declared there and referenced by the `windows` bindings — it is already in
//! the tree, pinned to the version `windows` itself resolves.
//!
//! Never call these on a UI thread. Every caller is a Tauri command running on
//! a worker.

use windows::core::{Result as WinResult, RuntimeType};
use windows_future::{
    AsyncActionCompletedHandler, AsyncOperationCompletedHandler, AsyncStatus, IAsyncAction,
    IAsyncOperation,
};

/// Wait for an `IAsyncOperation<T>` and return its result.
///
/// # Errors
///
/// Propagates whatever the operation itself reports.
pub fn block_on<T: RuntimeType + 'static>(op: &IAsyncOperation<T>) -> WinResult<T> {
    if op.Status()? == AsyncStatus::Started {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        op.SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
            // A send failure means the receiver is already gone, which can only
            // happen if we stopped waiting. Nothing to do about it, and nothing
            // that should turn into an error for the caller.
            let _ = tx.send(());
            Ok(())
        }))?;
        // If the handler already fired between the status check and the
        // registration, `WinRT` invokes it immediately, so this does not hang.
        let _ = rx.recv();
    }
    op.GetResults()
}

/// Wait for an `IAsyncAction` to finish.
///
/// # Errors
///
/// Propagates whatever the action itself reports.
pub fn block_on_action(op: &IAsyncAction) -> WinResult<()> {
    if op.Status()? == AsyncStatus::Started {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        op.SetCompleted(&AsyncActionCompletedHandler::new(move |_, _| {
            let _ = tx.send(());
            Ok(())
        }))?;
        let _ = rx.recv();
    }
    op.GetResults()
}
