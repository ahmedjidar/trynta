// SPDX-License-Identifier: AGPL-3.0-or-later
// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    keyring_lib::run();
}
