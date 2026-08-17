use std::path::Path;

/// Where the EFF long wordlist is expected (SPEC-V1 §7.3).
const WORDLIST: &str = "assets/eff_large_wordlist.txt";

fn main() {
    // The passphrase generator needs the EFF long wordlist, which is a licensed
    // third-party asset and is not vendored yet — THIRD-PARTY-NOTICES.md records
    // its terms as unconfirmed, and §7.4's sibling rule makes redistribution
    // permission a precondition rather than a follow-up.
    //
    // `include_str!` on a missing file is a compile error, so presence is decided
    // here instead. The effect is that dropping the file in and rebuilding turns
    // the feature on, and until then `generator_passphrase` reports the asset as
    // unavailable rather than silently generating from a short list.
    println!("cargo::rustc-check-cfg=cfg(has_wordlist)");
    if Path::new(WORDLIST).exists() {
        println!("cargo::rustc-cfg=has_wordlist");
    }
    println!("cargo::rerun-if-changed={WORDLIST}");

    tauri_build::build();
}
