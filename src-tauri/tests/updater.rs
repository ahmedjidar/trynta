//! The update channel's configuration invariants (SPEC-V1 §7.7, CLAUDE.md §4.11, §4.12).
//!
//! The cadence and the version guard are unit-tested inside
//! `services::updater`. What is checked here is the part that lives in JSON, where
//! a mistake compiles cleanly and ships: an endpoint over plain HTTP, an endpoint
//! with no public key to verify against, or a `dangerous*` escape hatch left on
//! after a debugging session.
//!
//! These assertions are written so they hold **both** while the channel is
//! unconfigured and after it is configured. A test that asserted "endpoints is
//! empty" would have to be edited by the very commit most in need of a check, and
//! at that point it is not a test, it is a formality.

use std::path::{Path, PathBuf};

fn repo_file(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn json(relative: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(repo_file(relative))
        .unwrap_or_else(|e| panic!("read {relative}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {relative}: {e}"))
}

#[test]
fn an_endpoint_may_only_be_configured_with_a_public_key_to_verify_against() {
    let conf = json("tauri.conf.json");
    let updater = &conf["plugins"]["updater"];
    assert!(
        updater.is_object(),
        "the plugin is registered unconditionally, so its config must exist — \
         without it the app fails to start rather than reporting featureUnavailable"
    );

    let endpoints = updater["endpoints"]
        .as_array()
        .expect("endpoints must be an array");
    let pubkey = updater["pubkey"].as_str().expect("pubkey must be a string");

    if endpoints.is_empty() {
        // The unconfigured state. `app.updater()` returns EmptyEndpoints and
        // `update_check` reports featureUnavailable.
        return;
    }

    assert!(
        !pubkey.trim().is_empty(),
        "an endpoint is configured with an empty pubkey. Every artefact would be \
         unverifiable, and SPEC-V1 §7.7 requires the signature checked before \
         applying. Configure both or neither."
    );

    for endpoint in endpoints {
        let url = endpoint.as_str().expect("an endpoint must be a string");
        assert!(
            url.starts_with("https://"),
            "update endpoint {url:?} is not HTTPS. The manifest is signed, so a \
             plaintext fetch does not let an attacker forge a release — but it does \
             let one see and suppress the check, and it puts the version on the \
             wire for every observer."
        );
    }

    assert_eq!(
        conf["bundle"]["createUpdaterArtifacts"], true,
        "an endpoint is configured but the bundler emits no updater artefacts, so \
         the channel serves nothing. These two flags move together."
    );
}

#[test]
fn no_dangerous_escape_hatch_is_enabled() {
    let conf = json("tauri.conf.json");
    let updater = &conf["plugins"]["updater"];

    for flag in [
        "dangerousInsecureTransportProtocol",
        "dangerousAcceptInvalidCerts",
        "dangerousAcceptInvalidHostnames",
    ] {
        let value = &updater[flag];
        assert!(
            value.is_null() || value == false,
            "{flag} is enabled. Each one of these turns the update channel into a \
             path for delivering an attacker's binary, which is the single worst \
             thing that can happen to a password manager."
        );
    }
}

#[test]
fn the_webview_cannot_start_a_download_or_an_install() {
    // CLAUDE.md §4.12: least-privilege capabilities. The frontend reaches the
    // updater through our two commands, which apply the cadence and the version
    // guard. Granting the plugin's own JS permissions would route around both.
    let capabilities = repo_file("capabilities");
    let mut checked = 0;

    for entry in std::fs::read_dir(&capabilities).expect("capabilities directory") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read capability");
        assert!(
            !text.contains("updater:"),
            "{} grants an updater permission to the webview. Remove it; the \
             frontend calls update_check and update_install instead.",
            path.display()
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no capability files were read, so this test proved nothing"
    );
}

#[test]
fn the_running_version_is_the_one_the_bundle_declares() {
    let conf = json("tauri.conf.json");
    let declared = conf["version"].as_str().expect("bundle version");

    // `offerable` compares the endpoint's answer against CARGO_PKG_VERSION. If the
    // bundle ships a different number, every user is comparing against a version
    // they are not running: too low and they are offered an update they already
    // have, too high and a real patch is refused.
    assert_eq!(
        declared,
        keyring_lib::services::updater::current_version(),
        "tauri.conf.json's version and Cargo.toml's version have diverged"
    );
}

#[test]
fn the_plugin_can_deserialize_its_own_configuration() {
    // The plugin is registered unconditionally, and its `setup` deserializes
    // `plugins.updater` into this exact type. If that fails, the app does not
    // start — and nothing else catches it: CI builds the bundle but never launches
    // it, so a config typo would ship as a binary that opens no window.
    //
    // Uses the plugin's own `Config`, not a local mirror, so a field it renames or
    // makes required is a failing test rather than a broken release.
    let conf = json("tauri.conf.json");
    let raw = conf["plugins"]["updater"].clone();

    let parsed: Result<tauri_plugin_updater::Config, _> = serde_json::from_value(raw);
    let config = parsed.expect(
        "plugins.updater does not deserialize into tauri_plugin_updater::Config. \
         The app will fail to start. Note that `pubkey` is required even when the \
         channel is unconfigured — an empty string is the right value there, not an \
         absent key.",
    );

    assert!(
        !config.dangerous_insecure_transport_protocol
            && !config.dangerous_accept_invalid_certs
            && !config.dangerous_accept_invalid_hostnames,
        "checked again through the parsed type, in case a field is ever renamed \
         and the string assertions above start passing vacuously"
    );
}
