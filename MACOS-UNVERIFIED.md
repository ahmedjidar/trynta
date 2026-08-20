# macOS: written, never compiled

Every macOS-specific code path in Trynta, what it is supposed to do, and how to
verify it on real hardware.

**Status: nothing below has ever been compiled.** Not "lightly tested" — not tested,
and not built. ADD-005 makes Windows the verified platform for budget reasons
(private repo, free Actions minutes exhausted, macOS runners bill at 10×). Treat
every line of macOS code as unknown until a step in this document has passed.

To calibrate how unknown: earlier on 2026-08-17, code that had been read, reviewed
and locally linted took **two CI round trips just to build** on macOS — a
`crate-type` collision produced two instances of `keyring_store` in one test binary,
and a timing test failed because the runner had three cores. Neither was visible from
a Windows machine. Expect this list to be wrong in ways it does not predict.

## How to work through this

```bash
# 1. Does it build at all? Everything else is downstream of this.
cargo check --workspace --all-targets

# 2. The automated tests, including the macOS platform suite that has never run.
cargo test --workspace
cargo test -p keyring --test platform_macos -- --nocapture

# 3. The whole gate.
pnpm verify:v1

# 4. The manual items below, which no test can cover.
```

Then push a `v*` tag or run the workflow with `macos: true` to get the same on a
runner. `verify-v1 (macos-latest)` and `bundle (macos-latest)` are
`continue-on-error: true` so an unverified platform cannot block a tag — **read the
log, do not trust the green tick.** Delete `continue-on-error` when macOS is green
and stays green.

## Cross-compilation: measured, does not work

Attempted from Windows on 2026-08-17 with the `aarch64-apple-darwin` std component
installed:

| Command                                                       | Result                                                                                                                       |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `cargo check -p keyring-crypto --target aarch64-apple-darwin` | **succeeds** — pure Rust, no C                                                                                               |
| `cargo check -p keyring-store --target aarch64-apple-darwin`  | fails: `libsqlite3-sys` compiles bundled SQLite, needs clang + the Apple SDK                                                 |
| `cargo check -p keyring --target aarch64-apple-darwin`        | fails: `objc2-exception-helper` compiles `try_catch.m`; the local `cc` rejects `-arch arm64` and `-mmacosx-version-min=11.0` |

The exception helper arrives through `tauri → tao → dispatch2 → block2 → objc2` and
`tauri-plugin-dialog → rfd → dispatch2`. Our own `default-features = false` cannot
switch it off, because cargo unifies features across the whole graph.

**The gap this records:** the crate holding _all_ the macOS code is the one crate
that cannot be checked without an Apple SDK. `keyring-crypto` cross-checking cleanly
is worth knowing but proves nothing about `platform/macos/`. A real cross-check needs
osxcross or `cargo-zigbuild`; neither is set up, and neither would run the tests.

---

## A — Build and toolchain

| #   | Check                                        | Expected                      | How                                                                                 |
| --- | -------------------------------------------- | ----------------------------- | ----------------------------------------------------------------------------------- |
| A1  | The workspace compiles                       | no errors                     | `cargo check --workspace --all-targets`                                             |
| A2  | `clippy::pedantic` is clean                  | no warnings                   | `cargo clippy --workspace --all-targets -- -D warnings`                             |
| A3  | Every `unsafe` block still justifies itself  | `platform/macos/` only        | `pnpm check:unsafe`                                                                 |
| A4  | Universal binary builds                      | both arches present           | `pnpm tauri build --target universal-apple-darwin`, then `lipo -info` on the binary |
| A5  | Installer size budget                        | ≤ 20 MB target, 25 MB ceiling | `pnpm check:bundle-size 20 25`                                                      |
| A6  | The app actually launches and shows a window | window appears                | run the built `.app` — CI builds a bundle but never opens it                        |

**A6 is not a formality.** The updater plugin is registered unconditionally and
deserializes `plugins.updater` at startup; `src-tauri/tests/updater.rs` asserts that
config parses, but a plugin `setup` that fails on macOS for another reason ships as a
binary that opens no window.

## B — Touch ID (`platform/macos/touch_id.rs`)

The security property here is _when the Keychain destroys the item_. No compile can
tell you that, so most of this section is manual.

| #   | Check                                     | Expected                                                                                                                    | How                                                                                                                      |
| --- | ----------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| B1  | **Enrolment change invalidates the wrap** | after adding or removing a fingerprint, `unwrap_secret` returns `Invalidated` and the app falls back to the master password | enrol biometric unlock, add a fingerprint in System Settings → Touch ID, relaunch, try biometric unlock                  |
| B2  | Availability is honest                    | `is_available()` false with no enrolment, true with one                                                                     | `cargo test -p keyring --test platform_macos touch_id_availability -- --nocapture`, then compare against System Settings |
| B3  | The round trip works at all               | `enrol` then `unwrap_secret` prompts for Touch ID and returns the same bytes                                                | manual — the automated test cannot do this, it would block on a finger                                                   |
| B4  | **`errSecUserCanceled` is really `-128`** | dismissing the prompt gives `Cancelled`, not `Invalidated`                                                                  | enrol, trigger biometric unlock, press Cancel; if the UI says the enrolment is gone, the constant is wrong               |
| B5  | No passcode → no item                     | `enrol` fails rather than storing unprotected                                                                               | test account with no login password (hard; skip if impractical and record that)                                          |
| B6  | The item is `ThisDeviceOnly`              | it does not appear on another Mac with the same iCloud account                                                              | `security find-generic-password -s dev.trynta.desktop.biometric` on a second Mac                                         |

**B4 is the specific uncertainty.** `errSecItemNotFound` (`-25300`) was verified
against `security-framework-sys 2.17.0`'s own source. `errSecUserCanceled` is
**not defined in that crate**, so `-128` has no in-tree source to check against. It
is marked `// UNVERIFIED:` at its definition. Getting it wrong is not harmless: a
cancelled prompt would be reported as `Invalidated` and the UI would tell the user
their enrolment is gone when they simply pressed Cancel.

**B1 is the one that matters most.** It is the whole reason the code uses
`AccessControlOptions::BIOMETRY_CURRENT_SET` rather than `BIOMETRY_ANY`. If it does
not hold, adding a fingerprint silently grants that finger access to the vault, and
SPEC-V1 §5.1 relies on the OS for exactly this.

## C — Clipboard (`platform/macos/clipboard.rs`)

| #   | Check                                                              | Expected                                                                   | How                                                                                                                 |
| --- | ------------------------------------------------------------------ | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| C1  | The concealed markers land                                         | both types present on the pasteboard after a copy                          | `cargo test -p keyring --test platform_macos a_secret_copy_carries_every_concealed_type`                            |
| C2  | The password lands too                                             | `public.utf8-plain-text` present, and paste produces the password          | `cargo test -p keyring --test platform_macos a_secret_copy_is_readable_as_text`, then ⌘V into a text field          |
| C3  | **`org.nspasteboard.AutoGeneratedType` is the correct identifier** | a clipboard manager treats the copy as programmatic                        | install a clipboard manager that honours the convention (Maccy, Paste), copy a password, confirm it is not recorded |
| C4  | `declareTypes:owner:` really is enough                             | `setString_forType` returns true                                           | C1/C2 failing with an `Err(WriteFailed)` is the signature of this being wrong                                       |
| C5  | Ownership check works                                              | a clear after the user copies something else does not wipe their clipboard | `cargo test -p keyring --test platform_macos clearing_does_not_touch_a_write_that_is_not_ours`                      |
| C6  | Universal Clipboard                                                | the password does not appear on a nearby iPhone                            | copy with Handoff on, check the iPhone clipboard                                                                    |

**C3 is the specific uncertainty.** nspasteboard.org's own fetchable text quotes
`org.nspasteboard.ConcealedType` — which is the one doing the security-relevant
work — and names `org.nspasteboard.TransientType`, but I could not confirm the exact
spelling of `AutoGeneratedType` from a primary source. A wrong identifier here is
silent: it declares a type nothing reads. Marked `// UNVERIFIED:` at its definition.

**C4 is a design decision worth understanding before changing it.** Apple's
documentation for `setString:forType:` is a JavaScript SPA and could not be fetched,
so the question "must the type be declared first?" is unresolved from a primary
source. The code calls `declareTypes:owner:` with all three types up front, which is
required under the strict reading and harmless under the lenient one. Do not
"simplify" it back to `clearContents()` — that would also reopen the window where a
clipboard manager can read the password before the concealed marker exists.

**C6 has no code behind it.** There is no API to opt out of Universal Clipboard; the
concealed type is the only lever. If the password does cross to an iPhone, that is a
threat-model entry, not a bug to fix — record it in SECURITY.md.

## D — Keychain secure store (`platform/macos/keychain.rs`)

| #   | Check                                                  | Expected                                                         | How                                                                                                                                                                                                                                                                                                                                                             |
| --- | ------------------------------------------------------ | ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | Round trip                                             | store → load → delete, and a missing key reads as `None`         | `cargo test -p keyring --test platform_macos the_keychain_round_trips_a_secret`                                                                                                                                                                                                                                                                                 |
| D2  | Replace, not duplicate                                 | a second `store` under one key wins                              | `cargo test -p keyring --test platform_macos storing_twice_replaces_rather_than_failing`                                                                                                                                                                                                                                                                        |
| D3  | Nothing plaintext in our own data directory            | no sentinel found                                                | `cargo test -p keyring --test platform_macos a_keychain_item_is_not_plaintext_in_the_app_support_directory`                                                                                                                                                                                                                                                     |
| D4  | The keychain access group is the one the app can claim | Keychain reads and writes succeed under a Developer ID signature | `entitlements.plist` claimed `$(AppIdentifierPrefix)app.keyring.desktop` while `tauri.conf.json` declared `dev.trynta.desktop` — a rename miss, corrected 2026-08-20. A group the bundle identifier does not match is rejected by the Keychain, so **biometric unlock would have failed on the first real device.** Verify: sign, run, store and load a secret. |
| D4  | Codesigning does not break it                          | still works from the signed `.app`, not just `cargo test`        | run D1 against the built bundle; Keychain ACLs are identity-scoped and an unsigned binary and a signed one are different identities                                                                                                                                                                                                                             |

**D4 has no automated form and is the most likely macOS-only surprise.** Keychain
access is granted per code-signing identity. Tests pass under `cargo test` and the
shipped app prompts "Trynta wants to access the keychain" — or fails — because it
is a different identity. Re-check after any change to signing or entitlements.

There is deliberately no counterpart to Windows'
`a_corrupted_dpapi_blob_reads_as_unreadable_not_as_success`: we own the DPAPI file
and can flip a bit in it, but the Keychain is an opaque system database with no
supported way to corrupt one item.

## E — Paths, window chrome, keyboard

| #   | Check                 | Expected                                                                                                                                                                                                                                               | How                                                                                                                                                   |
| --- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| E1  | Data directory        | `~/Library/Application Support/Trynta`                                                                                                                                                                                                                 | `cargo test -p keyring --test platform_macos -- --nocapture`, read `app-support-files-scanned`                                                        |
| E2  | Modifier key          | hints render `Cmd`, never a hardcoded `⌘`                                                                                                                                                                                                              | `platform::modifier_key()`; SPEC-V1 §8 forbids the literal in source                                                                                  |
| E3  | Traffic lights        | native, and the window is draggable                                                                                                                                                                                                                    | manual, run the app                                                                                                                                   |
| E4  | `mlock` equivalent    | `platform::memory::lock_pages` succeeds for all three 32-byte session keys, or logs a warning and continues. Implemented 2026-08-20 in `src-tauri/src/platform/memory.rs`; the Windows half is tested, the `libc::mlock` half has never been compiled. | `cargo test -p keyring --lib platform::memory` and `cargo test -p keyring --test key_pages_locked`, then unlock and read the log for the warning path |
| E5  | Auto-lock on sleep    | locks when the lid closes                                                                                                                                                                                                                              | manual: unlock, close the lid, reopen                                                                                                                 |
| E6  | LaunchAgent autostart | survives a reboot                                                                                                                                                                                                                                      | manual, once the setting exists (run 3)                                                                                                               |

## G — Frontend delivery and the bundled typeface

`custom-protocol` was missing from `src-tauri/Cargo.toml`, so every build served the
frontend from `build.devUrl` (`http://localhost:1420`) instead of the embedded bundle.
It is now `default`, which changes what a **macOS** bundle loads as much as a Windows
one, and it has only been observed on Windows. The typeface is a new bundled asset on
the same path.

| #   | Check                              | Expected                                                                | How                                                                                                                                                          |
| --- | ---------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| G1  | Bundle serves the frontend         | The window loads `tauri://localhost`, **not** `http://localhost:1420`   | `pnpm tauri build`, launch the `.app` with no dev server running. A blank window means `custom-protocol` did not reach the macOS build                       |
| G2  | Manrope loads under WKWebView      | Text renders in Manrope, not the system font                            | in the app, `document.fonts` reports `Manrope … loaded`. Failure is silent — the fallback renders and the layout is ~8% narrow                               |
| G3  | `font-src 'self'` allows the woff2 | No CSP violation for `/fonts/manrope-*.woff2`                           | Safari Web Inspector attached to the WKWebView; a violation shows in the console. WKWebView and WebView2 differ on how `'self'` resolves for a custom scheme |
| G4  | woff2 in the `.app`                | `Contents/Resources/…/fonts/manrope-latin.woff2` exists in the bundle   | `find <Trynta.app> -name 'manrope-*.woff2'` after `pnpm tauri build`                                                                                         |
| G5  | AC18 under WKWebView               | an injected `<style>` is blocked and `adoptedStyleSheets` still applies | this is verified on WebView2 by `e2e/specs/theme.e2e.ts`; the WKWebView half has no harness — run the two probes by hand in the Web Inspector console        |

## H — Window chrome

Windows runs frameless (`decorations: false`) with the app drawing its own minimise,
maximise and close. macOS must **not** do that: `tauri.macos.conf.json` overrides the
window with `decorations: true`, `titleBarStyle: "Overlay"` and `hiddenTitle: true`, so the
OS keeps its real traffic lights and floats them over our content. That override file has
never been parsed by anything.

| #   | Check                            | Expected                                                                                                           | How                                                                                                                                                                     |
| --- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| H1  | Platform config is merged        | The window has native traffic lights and no custom minimise/maximise/close                                         | run the app. Three drawn buttons at the top right means `tauri.macos.conf.json` was not merged and the Windows config applied                                           |
| H2  | Traffic-light inset              | The lights do not overlap the "Trynta" wordmark                                                                    | visual. `TitleBar` reserves `--pad-traffic-lights` plus a 68px lead-in on macOS; if Apple's inset differs the wordmark needs a different offset                         |
| H3  | Title bar drags the window       | Press and move on the bar moves the window                                                                         | manual. `useDragRegion` calls `startDragging()`; the Windows path needed this because Tauri's `data-tauri-drag-region` was inert — check whether macOS behaves the same |
| H4  | Double-click the title bar       | Zooms/unzooms, per the user's "Prefer tabs / double-click to" setting                                              | manual. Ours always calls `toggleMaximize`, which ignores that setting — decide whether that is acceptable on macOS                                                     |
| H5  | Corner radius                    | The window wears the system radius, and no app-coloured square peeks past it                                       | visual, on a light desktop background. Windows leaves the corner to DWM for the same reason                                                                             |
| H6  | `isMaximized` on a zoomed window | The restore/maximise glyph is not rendered on macOS at all, so nothing to check — but `WindowFrame` still reads it | it drives the frame's hairline; a wrong answer there is cosmetic, not functional                                                                                        |

## K — The app icon

`src-tauri/icons/icon.icns` is generated by `scripts`-adjacent tooling in the brand
commit: PNG frames wrapped in an ICNS container written from the format description.
No Apple tool has read it. The Windows `.ico` is verified — Explorer renders it — and
the two containers share nothing but the PNGs inside them.

| #   | Check                    | Expected                                                     | How                                                                                                          |
| --- | ------------------------ | ------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------ |
| K1  | The container parses     | `iconutil` reads it without complaint                        | `iconutil -c iconset src-tauri/icons/icon.icns -o /tmp/out.iconset` — a malformed container fails here first |
| K2  | Every frame is present   | 16, 32, 64, 128, 256, 512 and 1024 all appear in the iconset | `ls /tmp/out.iconset`. Missing sizes mean the wrong OSType codes, not a bad image                            |
| K3  | Finder and the Dock      | The mark, not a generic app square                           | `pnpm tauri build`, then look at the `.app` in Finder and launch it                                          |
| K4  | Retina frames are chosen | The Dock uses the 2x frame rather than upscaling the 1x      | visual on a Retina panel; a soft icon means `ic08`/`ic09` are not being picked                               |

## J — Hide from screen capture

`settings_set` and the startup hook both call Tauri's `set_content_protected`. On
Windows that is `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` and is verified; on
macOS it is `NSWindow.sharingType = .none`, which no build of this has ever reached.

The two platforms fail differently, and that is the reason this section exists.
`WDA_EXCLUDEFROMCAPTURE` hands the compositor a blank region, so a recorder gets black.
`sharingType` is advisory to _screen sharing_ and has historically not covered every
capture path — `screencapture`, ScreenCaptureKit and QuickTime have each behaved
differently across releases. A setting whose label says "hide from screen capture" and
which only hides from some of them is worse than one that says what it does, so the
answer to J2 decides whether the macOS copy needs to change.

| #   | Check                           | Expected                                                                                                         | How                                                                                                                            |
| --- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| J1  | The call reaches the window     | Toggling the setting does not log `could not change the window's capture protection`                             | turn it on in Settings, then read the app's `tracing` output. Failure is deliberately non-fatal, so the log is the only signal |
| J2  | It covers the capture paths     | `⌘⇧4`, `screencapture -x`, QuickTime screen recording and a Zoom/Teams share all show a blank or excluded window | do all four with the setting on. Record which, if any, still show the vault — this is the finding that decides the copy        |
| J3  | It survives a relaunch          | The window is protected before the lock screen appears, not after unlock                                         | enable it, quit, relaunch, and capture the **lock screen**. The flag lives in `app_state` precisely so this works pre-unlock   |
| J4  | Turning it off really turns off | The window is capturable again without a relaunch                                                                | toggle off, capture again. `WDA_NONE` is immediate on Windows; check macOS does not need the window recreated                  |

## I — Interface scale

| #   | Check                      | Expected                                                 | How                                                                                                                                                             |
| --- | -------------------------- | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| I1  | `Cmd` +, - and 0           | Zoom steps and resets                                    | manual. The handler accepts `metaKey                                                                                                                            |     | ctrlKey`, so it should need no macOS-specific code — this is the assumption to test |
| I2  | `Cmd`+wheel                | Zooms, and the webview does not also zoom underneath it  | manual. The wheel listener is non-passive and calls `preventDefault`; WKWebView may still apply its own pinch zoom                                              |
| I3  | CSS `zoom` under WKWebView | Text is re-rendered at the new size, not a scaled bitmap | zoom to 1.4 and inspect a hairline. Safari added `zoom` support later than Chromium; if it degrades to a transform, the fix is to scale the token layer instead |

## F — Things that only exist on Windows so far

Not gaps in macOS code — gaps in the platform layer that will need a macOS half
written, and which must arrive with tests in `platform_macos.rs` from the first
commit.

- `platform_hello_enrolled.rs` has no macOS twin. The equivalent — Touch ID enrolled
  and working end to end — is B1/B3 above and is manual by nature.
- Screen-capture mitigation (`contentProtected`, ADD-002 Q11) is configured but the
  macOS behaviour of `NSWindowSharingNone` is unchecked.

---

## Appending to this file

**Whenever you write macOS code, add its rows here in the same commit.** A row needs:

- what the code is supposed to do, in one line;
- the exact command or manual step that proves it;
- what a failure looks like, if that is not obvious from the expectation.

If you are uncertain about an API, mark it `// UNVERIFIED: <what could be wrong>` at
the site and add a **specific** row here — not "check the Keychain works". A row that
does not name a command or a concrete observation is not a check, and this file is
only useful if every row can be executed.
