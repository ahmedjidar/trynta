// SPDX-License-Identifier: AGPL-3.0-or-later
//! A user-supplied icon survives save → lock → reopen (ADD-001 tier 2).
//!
//! The processing pipeline has its own unit tests; this is about the other half —
//! that what `custom_icon::process` produces is what comes back out of a vault that
//! has been closed and reopened, byte for byte.
//!
//! It matters because the icon is the first non-textual thing stored in an item's
//! metadata envelope. `postcard` is not self-describing, so a field appended in the
//! wrong place or a `Vec<u8>` encoded as something else does not fail loudly — it
//! decodes into neighbouring fields and produces a title made of image data. Round
//! -tripping through a real file, with a real lock in between, is the only way to
//! know the encoding is right.
//!
//! The reopen is a genuine one: the session is dropped and the file unlocked again
//! from the password, so nothing is being read out of a cache that happens to still
//! be warm.

use std::path::Path;

use keyring_lib::services::custom_icon::{self, MAX_STORED_BYTES};
use keyring_store::model::IconFormat;
use keyring_store::{ItemBody, ItemDraft, KdfParams, VaultFile};

const MASTER: &str = "icon-roundtrip-master-7Kd2Qm";

/// A PNG big enough that processing must actually resize it.
fn source_png() -> Vec<u8> {
    let mut img = image::RgbaImage::new(400, 200);
    // Not a flat fill: a flat image compresses to almost nothing and would pass a
    // round-trip test even if the resize silently produced a 1×1.
    for (x, y, px) in img.enumerate_pixels_mut() {
        *px = image::Rgba([
            u8::try_from(x % 256).unwrap_or(0),
            u8::try_from(y % 256).unwrap_or(0),
            u8::try_from((x + y) % 256).unwrap_or(0),
            255,
        ]);
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode source");
    out
}

const SOURCE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><!-- editor comment --><metadata>cruft</metadata><circle cx="16" cy="16" r="15" fill="#3F4CA8"/></svg>"##;

fn seed_item(path: &Path) -> uuid::Uuid {
    let file = VaultFile::create(path, MASTER, KdfParams::floor()).expect("create");
    let session = file.unlock(MASTER).expect("unlock");
    let vault = session
        .vault_add("Personal", "vault.accent.1")
        .expect("vault");
    session
        .item_upsert(&ItemDraft::new(
            vault,
            "an item with an icon",
            ItemBody::Login {
                username: "alice@example.test".to_owned(),
                password: "FIXTURE-PASSWORD-Zq8Wt".to_owned(),
                urls: vec!["https://example.test".to_owned()],
                totp: None,
            },
        ))
        .expect("item")
}

#[test]
fn a_processed_raster_survives_save_and_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let id = seed_item(&path);

    let processed = custom_icon::process(&source_png()).expect("processed");
    assert!(processed.bytes <= MAX_STORED_BYTES);
    let stored = processed.icon.clone();

    {
        let file = VaultFile::open(&path).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        assert!(
            session
                .item_set_custom_icon(id, Some(stored.clone()))
                .expect("set"),
            "attaching an icon is a change"
        );
        // Setting the identical bytes again must not burn a revision — the manifest
        // uses `revision` to detect a rollback, and churning it is noise in that signal.
        assert!(
            !session
                .item_set_custom_icon(id, Some(stored.clone()))
                .expect("set again"),
            "re-setting the same icon is a no-op"
        );
    }

    // A real reopen: new file handle, unlocked from the password.
    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("unlock");
    let read = session
        .item_custom_icon(id)
        .expect("read")
        .expect("present");

    assert_eq!(read.format, stored.format, "format survived");
    assert_eq!(read.bytes, stored.bytes, "bytes survived exactly");

    // And it is still a decodable image at the size the pipeline promised.
    let decoded = image::load_from_memory(&read.bytes).expect("still an image");
    assert_eq!(decoded.width(), 128);
    assert_eq!(decoded.height(), 64);

    // The rest of the item is untouched: the icon must not have displaced a field.
    let meta = session.item_meta(id).expect("meta");
    assert_eq!(meta.title, "an item with an icon");
    assert!(meta.has_custom_icon);
    let password = session
        .item_secret(id, keyring_store::SecretField::Password)
        .expect("password");
    assert_eq!(password.as_str(), "FIXTURE-PASSWORD-Zq8Wt");
}

#[test]
fn a_sanitised_svg_survives_save_and_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let id = seed_item(&path);

    let processed = custom_icon::process(SOURCE_SVG.as_bytes()).expect("processed");
    assert_eq!(processed.icon.format, IconFormat::Svg);

    {
        let file = VaultFile::open(&path).expect("open");
        let session = file.unlock(MASTER).expect("unlock");
        session
            .item_set_custom_icon(id, Some(processed.icon.clone()))
            .expect("set");
    }

    let file = VaultFile::open(&path).expect("reopen");
    let session = file.unlock(MASTER).expect("unlock");
    let read = session
        .item_custom_icon(id)
        .expect("read")
        .expect("present");
    assert_eq!(read.bytes, processed.icon.bytes);

    let text = String::from_utf8(read.bytes).expect("utf8");
    assert!(text.contains("<circle"), "the drawing survived: {text}");
    assert!(
        !text.contains("editor comment"),
        "the comment did not: {text}"
    );
    assert!(!text.contains("cruft"), "the metadata did not: {text}");
}

#[test]
fn clearing_an_icon_removes_it_and_leaves_the_item() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let id = seed_item(&path);

    let processed = custom_icon::process(&source_png()).expect("processed");
    let file = VaultFile::open(&path).expect("open");
    let session = file.unlock(MASTER).expect("unlock");
    session
        .item_set_custom_icon(id, Some(processed.icon))
        .expect("set");
    assert!(session.item_meta(id).expect("meta").has_custom_icon);

    assert!(session.item_set_custom_icon(id, None).expect("clear"));
    assert!(session.item_custom_icon(id).expect("read").is_none());
    assert!(!session.item_meta(id).expect("meta").has_custom_icon);
    // Clearing twice is a no-op rather than an error or a revision bump.
    assert!(!session.item_set_custom_icon(id, None).expect("clear again"));

    let meta = session.item_meta(id).expect("meta");
    assert_eq!(meta.title, "an item with an icon");
}

#[test]
fn an_item_with_no_icon_reads_back_as_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("vault.db");
    let id = seed_item(&path);

    let file = VaultFile::open(&path).expect("open");
    let session = file.unlock(MASTER).expect("unlock");
    assert!(session.item_custom_icon(id).expect("read").is_none());
    assert!(!session.item_meta(id).expect("meta").has_custom_icon);
}
