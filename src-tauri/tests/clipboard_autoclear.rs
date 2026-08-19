//! Auto-clear actually clears, checked by reading the clipboard back.
//!
//! The bug this exists for: `item_copy_field` wrote the secret, stored the ownership
//! token so a clear *could* tell its own write from the user's, and then nothing ever
//! scheduled one. The setting was on by default and said "copied secrets are wiped
//! after 30 seconds". The value stayed on the system clipboard until something else
//! replaced it — reported from a real session as "still pasteable well beyond that".
//!
//! No test caught it because there was nothing to catch: the only observable was that
//! a timer had been asked to run, and nobody had asked. So these tests do the one
//! thing that cannot be faked into passing — they **read the clipboard** and look.
//!
//! Windows only (ADD-005). The macOS half is `NSPasteboard` and has never been built;
//! `MACOS-UNVERIFIED.md` C-section carries it.
//!
//! Serialised with a mutex: there is one system clipboard, and two of these running at
//! once would each see the other's writes.

#![cfg(windows)]

use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use keyring_lib::platform::clipboard::Clipboard;
use keyring_lib::platform::windows::clipboard::{read_text, WindowsClipboard};

/// One test at a time. There is a single clipboard on the machine.
fn clipboard_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A value no other process would put on a clipboard.
const SENTINEL: &str = "keyring-autoclear-sentinel-9f2a4c7e-do-not-paste";

/// Leave the machine's clipboard in a defined state rather than holding a sentinel.
fn tidy(clipboard: &WindowsClipboard) {
    let token = clipboard.set_secret("keyring test finished").unwrap_or(0);
    let _ = clipboard.clear_if_ours(token);
}

#[test]
fn a_written_secret_is_readable_and_a_cleared_one_is_not() {
    let _guard = clipboard_lock().lock().expect("clipboard lock");
    let clipboard = WindowsClipboard::new();

    let token = clipboard.set_secret(SENTINEL).expect("write");
    assert_eq!(
        read_text().as_deref(),
        Some(SENTINEL),
        "the sentinel was not on the clipboard, so nothing after this proves anything"
    );

    let cleared = clipboard.clear_if_ours(token).expect("clear");
    assert!(cleared, "our own write should have been recognised as ours");
    assert_ne!(
        read_text().as_deref(),
        Some(SENTINEL),
        "the secret survived a clear"
    );

    tidy(&clipboard);
}

#[test]
fn a_scheduled_clear_removes_the_value_after_the_interval() {
    let _guard = clipboard_lock().lock().expect("clipboard lock");
    let clipboard = Arc::new(WindowsClipboard::new());

    let token = clipboard.set_secret(SENTINEL).expect("write");
    assert_eq!(read_text().as_deref(), Some(SENTINEL), "fixture");

    // The same shape `schedule_clipboard_clear` uses: a thread, a sleep, then a
    // token-scoped clear. One second rather than the product's default, because a
    // test that waited thirty would be a test nobody runs.
    let interval = Duration::from_secs(1);
    let worker = {
        let clipboard = Arc::clone(&clipboard);
        std::thread::spawn(move || {
            std::thread::sleep(interval);
            clipboard.clear_if_ours(token)
        })
    };

    // Still there before the interval elapses. This is the half that would have failed
    // if a fix "cleared" eagerly and called it auto-clear.
    std::thread::sleep(Duration::from_millis(250));
    assert_eq!(
        read_text().as_deref(),
        Some(SENTINEL),
        "the value went early — a copy has to survive long enough to be pasted"
    );

    let cleared = worker.join().expect("worker").expect("clear");
    assert!(cleared, "the scheduled clear did not recognise our write");
    assert_ne!(
        read_text().as_deref(),
        Some(SENTINEL),
        "the secret was still on the clipboard after the interval — this is the bug"
    );

    tidy(&clipboard);
}

#[test]
fn a_clear_leaves_something_the_user_copied_afterwards_alone() {
    let _guard = clipboard_lock().lock().expect("clipboard lock");
    let clipboard = WindowsClipboard::new();

    // Our secret, then the user copies their own thing.
    let ours = clipboard.set_secret(SENTINEL).expect("write");
    let theirs = "a shopping list the user would be annoyed to lose";
    let _ = clipboard.set_secret(theirs).expect("user write");

    // The timer for *our* write fires late. It must do nothing.
    let cleared = clipboard.clear_if_ours(ours).expect("clear");
    assert!(
        !cleared,
        "a stale timer wiped the clipboard after the user had copied something else"
    );
    assert_eq!(
        read_text().as_deref(),
        Some(theirs),
        "the user's value should still be there"
    );

    tidy(&clipboard);
}
