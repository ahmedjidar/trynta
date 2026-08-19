// SPDX-License-Identifier: AGPL-3.0-or-later
//! Processing a user-supplied icon (ADD-001 tier 2).
//!
//! Everything here runs in Rust and nothing runs in the webview. The webview never sees
//! the file the user picked: it asks for a path, Rust reads, decodes, resizes,
//! re-encodes and hands back only the processed bytes and their size. That is the same
//! boundary the clipboard path uses, and for a related reason — a decoder is a parser,
//! and parsers are where untrusted input turns into arbitrary behaviour.
//!
//! ## What arrives, and what leaves
//!
//! | In | Out |
//! |---|---|
//! | PNG, JPEG, WebP, ICO | one lossless WebP, or an optimised PNG if WebP will not encode |
//! | SVG | an allowlisted, re-serialised SVG |
//!
//! Rasters are resized to fit a 128×128 box with the aspect ratio preserved. The design
//! draws tiles at 32–56px, so 128 covers a 3× display with no visible loss and a larger
//! source buys nothing but bytes in the vault.
//!
//! **No metadata survives.** Re-encoding from decoded pixels is what does it: EXIF, XMP,
//! ICC and every other ancillary chunk live in the container, and the container is
//! thrown away. A holiday photo's GPS coordinates must never end up inside a vault
//! because someone used it as an icon.
//!
//! ## The SVG rules are the theme validator's rules
//!
//! SPEC-V1 §7.6 settled this class of question for imported themes: *"`background:
//! url(https://attacker/…)` is a network beacon that fires on render — precisely the
//! leak ADD-001 exists to prevent. Validate **in Rust**, not the webview."* An SVG is
//! the same problem with more surface, so it gets the same answer: an allowlist over a
//! real XML tokeniser, and anything not on the list is a rejection rather than a strip.
//!
//! A regex would not do. `<script` can arrive entity-encoded, inside CDATA, or split by
//! a comment, and a pattern that catches all three is a parser written badly.
//!
//! Rejected outright: `<script>`, `<foreignObject>`, `<image>`, every animation element,
//! any `on*` attribute, any `href` that is not a same-document `#fragment`, any `url()`
//! or `@import` in a `<style>` or a presentation attribute, and any DOCTYPE at all —
//! the last one because an external entity is how an XML parser is talked into reading
//! a file off disk.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};
use keyring_store::model::{IconFormat, StoredIcon};
use quick_xml::events::attributes::Attribute;
use quick_xml::events::Event;
use quick_xml::{Reader, Writer};

/// Largest file accepted, checked before a decoder is handed anything.
///
/// A logo is kilobytes. Two megabytes is generous for a source PNG and small enough that
/// refusing costs nothing — and refusing on length is the only check that is free.
pub const MAX_UPLOAD_BYTES: usize = 2 * 1024 * 1024;

/// Largest processed result stored on an item.
pub const MAX_STORED_BYTES: usize = 64 * 1024;

/// Longest edge of the processed raster.
const TARGET_EDGE: u32 = 128;

/// Ceiling on decoded pixels, to refuse a decompression bomb before it is allocated.
///
/// 64 megapixels is far beyond any logo and still an order of magnitude below the point
/// where a 4-byte-per-pixel buffer becomes a problem.
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// Why an icon was refused.
///
/// Every variant names something about the *file*. None of them can carry a fragment of
/// its contents, which is the property that keeps a rejected upload out of an error
/// string and therefore out of a log (CLAUDE.md §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IconError {
    /// Larger than [`MAX_UPLOAD_BYTES`] before decoding.
    #[error("the file is larger than 2 MB")]
    TooLarge,
    /// Not one of the accepted formats, or corrupt.
    #[error("the file is not a PNG, JPEG, WebP, ICO or SVG")]
    Unsupported,
    /// Decoded, but implausibly large.
    #[error("the image's dimensions are implausible")]
    Implausible,
    /// An SVG containing something the allowlist refuses.
    #[error("the SVG contains script, an external reference, or an unsupported element")]
    UnsafeSvg,
    /// Processed successfully but still over [`MAX_STORED_BYTES`].
    #[error("the processed icon is larger than 64 KB")]
    StillTooLarge,
    /// The image could not be re-encoded.
    #[error("the image could not be processed")]
    Encode,
}

/// A processed icon and what it cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processed {
    /// Ready to store on the item.
    pub icon: StoredIcon,
    /// Byte length of the processed result, for the picker to display.
    pub bytes: usize,
}

/// Process an uploaded file into something storable.
///
/// # Errors
///
/// [`IconError`], every variant of which describes the file rather than its contents.
pub fn process(raw: &[u8]) -> Result<Processed, IconError> {
    if raw.len() > MAX_UPLOAD_BYTES {
        return Err(IconError::TooLarge);
    }
    if raw.is_empty() {
        return Err(IconError::Unsupported);
    }

    let icon = if looks_like_svg(raw) {
        let text = std::str::from_utf8(raw).map_err(|_| IconError::Unsupported)?;
        StoredIcon {
            format: IconFormat::Svg,
            bytes: sanitise_svg(text)?.into_bytes(),
        }
    } else {
        process_raster(raw)?
    };

    let bytes = icon.bytes.len();
    if bytes > MAX_STORED_BYTES {
        return Err(IconError::StillTooLarge);
    }
    Ok(Processed { icon, bytes })
}

/// Whether the bytes are plausibly SVG.
///
/// Sniffed rather than trusted from a file extension: the extension is chosen by whoever
/// made the file. Leading whitespace and a BOM are skipped because both are common.
fn looks_like_svg(raw: &[u8]) -> bool {
    let trimmed = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    let head: &[u8] = &trimmed[..trimmed.len().min(1024)];
    let Ok(text) = std::str::from_utf8(head) else {
        return false;
    };
    let lower = text.trim_start().to_ascii_lowercase();
    lower.starts_with("<?xml") || lower.starts_with("<svg") || lower.starts_with("<!--")
}

// ── raster ───────────────────────────────────────────────────────────────────

/// Decode, resize and re-encode a raster.
fn process_raster(raw: &[u8]) -> Result<StoredIcon, IconError> {
    let mut reader = ImageReader::new(Cursor::new(raw))
        .with_guessed_format()
        .map_err(|_| IconError::Unsupported)?;

    // Only the four formats the picker offers. `with_guessed_format` will happily
    // identify others, and a format we did not intend to accept is a decoder we did not
    // intend to expose.
    match reader.format() {
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Ico) => {}
        _ => return Err(IconError::Unsupported),
    }

    // Bound the allocation before the decoder makes it. Without this a 40 KB PNG can
    // declare a 60,000 × 60,000 canvas and ask for 14 GB.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);

    let decoded = reader.decode().map_err(|_| IconError::Unsupported)?;
    let (w, h) = (decoded.width(), decoded.height());
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(IconError::Implausible);
    }

    // `resize` fits inside the box and keeps the aspect ratio; it never upscales past
    // the source's own size in a way that would invent detail, and an already-small
    // logo is left alone.
    let resized = if w > TARGET_EDGE || h > TARGET_EDGE {
        decoded.resize(
            TARGET_EDGE,
            TARGET_EDGE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        decoded
    };

    // RGBA8 for both encoders: a paletted or 16-bit source would otherwise round-trip
    // through whatever the encoder happens to support, and transparency is the one thing
    // a logo cannot lose.
    let rgba = resized.to_rgba8();

    if let Some(bytes) = encode_webp(&rgba) {
        return Ok(StoredIcon {
            format: IconFormat::Webp,
            bytes,
        });
    }

    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(rgba.clone())
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|_| IconError::Encode)?;
    Ok(StoredIcon {
        format: IconFormat::Png,
        bytes: png,
    })
}

/// Encode losslessly as WebP, or `None` if this build cannot.
///
/// Separated so the fallback is a branch rather than an error: ADD-001's brief asks for
/// WebP "falling back to optimised PNG", and a build without a WebP encoder must still
/// accept icons.
fn encode_webp(rgba: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = image::codecs::webp::WebPEncoder::new_lossless(Cursor::new(&mut out));
    encoder
        .encode(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    (!out.is_empty()).then_some(out)
}

// ── SVG ──────────────────────────────────────────────────────────────────────

/// Elements a logo may contain.
///
/// Shape, grouping, gradient and clipping primitives. Deliberately absent: `<image>`
/// (a raster inside a vector, and a href to fetch it with), `<foreignObject>` (arbitrary
/// HTML), `<script>`, `<a>` (navigable), every `animate*`/`set` element (they assign
/// attributes at runtime, which turns any allowlist into a suggestion), and `<switch>`.
const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "symbol",
    "use",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "clippath",
    "mask",
    "pattern",
    "lineargradient",
    "radialgradient",
    "stop",
    "style",
    "title",
    "desc",
    "metadata",
];

/// Elements dropped along with their contents rather than rejected.
///
/// These are editor bookkeeping, not drawing. Removing them *is* the optimisation ADD-001
/// asks for — a Illustrator export is routinely half metadata.
const DROPPED_ELEMENTS: &[&str] = &["metadata", "title", "desc"];

/// Attributes that may carry a reference, checked rather than allowed.
const REFERENCE_ATTRS: &[&str] = &[
    "href",
    "xlink:href",
    "clip-path",
    "mask",
    "fill",
    "stroke",
    "filter",
];

/// Sanitise and re-serialise an SVG.
///
/// The output is built by writing out only the events that survive, so what is emitted
/// is what was inspected — there is no path by which an unexamined byte reaches the
/// result.
fn sanitise_svg(source: &str) -> Result<String, IconError> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;

    let mut writer = Writer::new(Vec::new());
    let mut depth_dropped: Option<Vec<u8>> = None;
    let mut saw_svg = false;

    loop {
        match reader.read_event() {
            // Three ways a file stops being a logo, refused together because the
            // response is the same and the response is the point:
            //
            // - a **parse error** — a sanitiser that cannot read the document cannot
            //   vouch for it, and CLAUDE.md §4.10 says fail closed;
            // - a **DOCTYPE** — how an XML parser is asked to read a file off disk
            //   (billion laughs, XXE). No logo needs one;
            // - **CDATA** — a way to smuggle markup past a naive filter. Refused
            //   rather than unwrapped.
            Err(_) | Ok(Event::DocType(_) | Event::CData(_)) => return Err(IconError::UnsafeSvg),

            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                let local = local_name(&name);
                if depth_dropped.is_some() {
                    continue;
                }
                if DROPPED_ELEMENTS.contains(&local.as_str()) {
                    depth_dropped = Some(name.clone());
                    continue;
                }
                if !ALLOWED_ELEMENTS.contains(&local.as_str()) {
                    return Err(IconError::UnsafeSvg);
                }
                if local == "svg" {
                    saw_svg = true;
                }
                let cleaned = clean_element(&e)?;
                writer
                    .write_event(Event::Start(cleaned))
                    .map_err(|_| IconError::Encode)?;
            }

            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                let local = local_name(&name);
                if depth_dropped.is_some() {
                    continue;
                }
                if DROPPED_ELEMENTS.contains(&local.as_str()) {
                    continue;
                }
                if !ALLOWED_ELEMENTS.contains(&local.as_str()) {
                    return Err(IconError::UnsafeSvg);
                }
                let cleaned = clean_element(&e)?;
                writer
                    .write_event(Event::Empty(cleaned))
                    .map_err(|_| IconError::Encode)?;
            }

            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_ascii_lowercase();
                if let Some(open) = &depth_dropped {
                    if *open == name {
                        depth_dropped = None;
                    }
                    continue;
                }
                writer
                    .write_event(Event::End(e))
                    .map_err(|_| IconError::Encode)?;
            }

            // Text inside `<style>` is CSS, and CSS is where `url()` and `@import` live.
            Ok(Event::Text(t)) => {
                if depth_dropped.is_some() {
                    continue;
                }
                let text = t.into_inner();
                let lowered = String::from_utf8_lossy(&text).to_ascii_lowercase();
                if lowered.contains("url(")
                    || lowered.contains("@import")
                    || lowered.contains("javascript:")
                {
                    return Err(IconError::UnsafeSvg);
                }
                writer
                    .write_event(Event::Text(quick_xml::events::BytesText::from_escaped(
                        String::from_utf8_lossy(&text).into_owned(),
                    )))
                    .map_err(|_| IconError::Encode)?;
            }

            // Comments, processing instructions and the XML declaration: cruft, and
            // dropping them is part of the optimisation. Same arm as the catch-all
            // because they do the same thing — nothing — and a future quick-xml event
            // this code has never seen should be dropped too rather than written
            // through unexamined.
            Ok(_) => {}
        }
    }

    if !saw_svg {
        return Err(IconError::Unsupported);
    }

    let out = writer.into_inner();
    String::from_utf8(out).map_err(|_| IconError::Encode)
}

/// The local part of a possibly namespaced element name, lowercased.
fn local_name(name: &[u8]) -> String {
    let text = String::from_utf8_lossy(name);
    match text.rsplit_once(':') {
        Some((_, local)) => local.to_ascii_lowercase(),
        None => text.to_ascii_lowercase(),
    }
}

/// Copy an element's attributes, refusing the dangerous ones.
fn clean_element<'a>(
    e: &'a quick_xml::events::BytesStart<'a>,
) -> Result<quick_xml::events::BytesStart<'a>, IconError> {
    let mut out =
        quick_xml::events::BytesStart::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());

    for attr in e.attributes().with_checks(false) {
        let Attribute { key, value } = attr.map_err(|_| IconError::UnsafeSvg)?;
        let name = String::from_utf8_lossy(key.as_ref()).to_ascii_lowercase();
        let raw = String::from_utf8_lossy(value.as_ref()).to_string();
        let lowered = raw.to_ascii_lowercase();

        // Every event handler in SVG is spelled `on…`. There is no allowlist of safe
        // ones because there are none.
        if name.starts_with("on") {
            return Err(IconError::UnsafeSvg);
        }
        // A namespace declaration pointing anywhere but SVG/xlink is a way to smuggle
        // foreign markup that a namespace-aware renderer will honour.
        //
        // Checked and then `continue`d, deliberately: the two legal values are absolute
        // URLs, and the generic "no scheme anywhere" rule below would otherwise reject
        // every valid SVG ever written. This is the one attribute where a URL is the
        // correct content, so it is the one attribute allowed to skip that rule — and it
        // only gets there by matching one of exactly two constants.
        if name == "xmlns" || name.starts_with("xmlns:") {
            if !(lowered == "http://www.w3.org/2000/svg"
                || lowered == "http://www.w3.org/1999/xlink")
            {
                return Err(IconError::UnsafeSvg);
            }
            out.push_attribute(Attribute {
                key,
                value: value.clone(),
            });
            continue;
        }
        if lowered.contains("javascript:") || lowered.contains("@import") {
            return Err(IconError::UnsafeSvg);
        }
        if REFERENCE_ATTRS.contains(&name.as_str()) {
            // `url(#id)` and `#id` are same-document. Anything else — an http URL, a
            // `data:` payload, a bare path — is a fetch, and ADD-001 forbids the fetch.
            let trimmed = lowered.trim().to_owned();
            if trimmed.starts_with("url(") {
                let inner = trimmed
                    .trim_start_matches("url(")
                    .trim_end_matches(')')
                    .trim_matches(['"', '\'', ' ']);
                if !inner.starts_with('#') {
                    return Err(IconError::UnsafeSvg);
                }
            } else if (name == "href" || name == "xlink:href") && !trimmed.starts_with('#') {
                return Err(IconError::UnsafeSvg);
            }
        }
        // Anything else that manages to name a scheme is refused on principle.
        if lowered.contains("://") {
            return Err(IconError::UnsafeSvg);
        }

        out.push_attribute(Attribute {
            key,
            value: value.clone(),
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest valid PNG the `image` crate will produce, for the accept cases.
    fn tiny_png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .expect("encode");
        out
    }

    // ── size and format gates ────────────────────────────────────────────────

    #[test]
    fn an_oversized_file_is_refused_before_decoding() {
        let huge = vec![0u8; MAX_UPLOAD_BYTES + 1];
        assert_eq!(process(&huge), Err(IconError::TooLarge));
    }

    #[test]
    fn an_empty_or_unknown_file_is_refused() {
        assert_eq!(process(&[]), Err(IconError::Unsupported));
        assert_eq!(process(b"not an image at all"), Err(IconError::Unsupported));
    }

    #[test]
    fn a_png_is_re_encoded_and_downscaled() {
        let processed = process(&tiny_png(512, 256)).expect("accepted");
        assert!(matches!(
            processed.icon.format,
            IconFormat::Webp | IconFormat::Png
        ));
        assert_eq!(processed.bytes, processed.icon.bytes.len());
        assert!(processed.bytes <= MAX_STORED_BYTES);

        // Decoding the result proves the aspect ratio survived and the long edge landed
        // on the target.
        let out = image::load_from_memory(&processed.icon.bytes).expect("decodes");
        assert_eq!(out.width(), TARGET_EDGE);
        assert_eq!(out.height(), TARGET_EDGE / 2);
    }

    #[test]
    fn a_small_image_is_not_upscaled() {
        let processed = process(&tiny_png(32, 32)).expect("accepted");
        let out = image::load_from_memory(&processed.icon.bytes).expect("decodes");
        assert_eq!((out.width(), out.height()), (32, 32));
    }

    /// A JPEG carrying an APP1 `Exif` segment with a GPS IFD pointer.
    ///
    /// Built by splicing the segment in after SOI rather than by pulling in an EXIF
    /// writer: the point of the test is that whatever arrives is gone afterwards, and a
    /// hand-built segment is the version of that a reviewer can check by eye.
    fn jpeg_with_gps_exif() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(200, 200, image::Rgba([90, 40, 10, 255]));
        let mut base = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .to_rgb8()
            .write_to(&mut Cursor::new(&mut base), ImageFormat::Jpeg)
            .expect("encode");

        // TIFF header, one IFD entry (GPSInfo, tag 0x8825), then a GPS IFD holding a
        // latitude reference. `SECRET_PLACE` stands in for the coordinates themselves.
        let mut tiff: Vec<u8> = Vec::new();
        tiff.extend_from_slice(b"II\x2a\x00\x08\x00\x00\x00"); // little-endian, IFD0 at 8
        tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry
        tiff.extend_from_slice(&0x8825u16.to_le_bytes()); // GPSInfo
        tiff.extend_from_slice(&4u16.to_le_bytes()); // LONG
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count
        tiff.extend_from_slice(&26u32.to_le_bytes()); // offset of the GPS IFD
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD: none
        tiff.extend_from_slice(b"SECRET_PLACE\x00");

        let mut app1: Vec<u8> = Vec::new();
        app1.extend_from_slice(b"Exif\x00\x00");
        app1.extend_from_slice(&tiff);
        let len = u16::try_from(app1.len() + 2).expect("segment fits");

        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(&base[..2]); // SOI
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&base[2..]);
        out
    }

    #[test]
    fn exif_and_gps_metadata_never_reach_the_vault() {
        let source = jpeg_with_gps_exif();
        // The fixture is only meaningful if the tags really are in the input.
        assert!(
            source.windows(6).any(|w| w == b"Exif\x00\x00"),
            "fixture is not carrying an EXIF segment"
        );
        assert!(
            source.windows(12).any(|w| w == b"SECRET_PLACE"),
            "fixture is not carrying the GPS payload"
        );

        let processed = process(&source).expect("a JPEG with EXIF is still a valid upload");
        let out = &processed.icon.bytes;

        assert!(
            !out.windows(4).any(|w| w == b"Exif"),
            "an EXIF segment survived into the stored icon"
        );
        assert!(
            !out.windows(12).any(|w| w == b"SECRET_PLACE"),
            "a GPS tag survived into the stored icon"
        );
        // Nothing of the original container survives either. The stored icon is not a
        // JPEG at all — the pixels were decoded and re-encoded, so there is no path by
        // which an unexamined ancillary segment could be copied across. Asserting on the
        // container is the check that means something; scanning for a bare `FF E1` pair
        // is not, because that is only a marker inside a JPEG and is ordinary data
        // anywhere in a compressed stream.
        assert!(
            matches!(processed.icon.format, IconFormat::Webp | IconFormat::Png),
            "the stored icon must be re-encoded, not passed through"
        );
        assert_ne!(
            image::guess_format(out).expect("the result is a decodable image"),
            ImageFormat::Jpeg,
            "the original JPEG container was carried over"
        );
        assert!(out.len() <= MAX_STORED_BYTES);
    }

    // ── the SVG rejection cases ──────────────────────────────────────────────

    fn svg(body: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">{body}</svg>"#)
    }

    #[test]
    fn a_plain_svg_survives() {
        let out = process(svg(r##"<path d="M0 0h24v24H0z" fill="#123456"/>"##).as_bytes())
            .expect("accepted");
        assert_eq!(out.icon.format, IconFormat::Svg);
        let text = String::from_utf8(out.icon.bytes).expect("utf8");
        assert!(text.contains("<path"));
        assert!(text.contains("viewBox"));
    }

    #[test]
    fn script_is_rejected() {
        for body in [
            "<script>alert(1)</script>",
            "<script/>",
            r#"<g><script type="text/javascript">x</script></g>"#,
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn event_handlers_are_rejected() {
        for body in [
            r#"<path d="M0 0" onload="x()"/>"#,
            r#"<circle cx="1" cy="1" r="1" onclick="x()"/>"#,
            r#"<g onmouseover="x()"><path d="M0 0"/></g>"#,
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn foreign_object_and_image_are_rejected() {
        for body in [
            "<foreignObject><div>hi</div></foreignObject>",
            r#"<image href="https://attacker.example/x.png"/>"#,
            r#"<image xlink:href="data:image/png;base64,AAAA"/>"#,
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn external_references_are_rejected() {
        for body in [
            r#"<use href="https://attacker.example/x.svg#a"/>"#,
            r#"<use xlink:href="http://attacker.example/x.svg#a"/>"#,
            r#"<path d="M0 0" fill="url(https://attacker.example/g.svg#grad)"/>"#,
            r#"<path d="M0 0" clip-path="url(http://attacker.example/#c)"/>"#,
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn internal_references_survive() {
        let body = r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#fff"/></linearGradient></defs><path d="M0 0" fill="url(#g)"/><use href="#g"/>"##;
        let out = process(svg(body).as_bytes()).expect("accepted");
        let text = String::from_utf8(out.icon.bytes).expect("utf8");
        assert!(text.contains("url(#g)"));
    }

    #[test]
    fn css_imports_and_urls_are_rejected() {
        for body in [
            "<style>@import url('https://attacker.example/x.css');</style>",
            "<style>.a{background:url(https://attacker.example/x.png)}</style>",
            "<style>.a{background:url(http://x/y)}</style>",
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn a_doctype_is_rejected() {
        let raw = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY x SYSTEM "file:///etc/passwd">]><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="M0 0"/></svg>"#;
        assert_eq!(process(raw.as_bytes()), Err(IconError::UnsafeSvg));
    }

    #[test]
    fn cdata_is_rejected() {
        let body = "<style><![CDATA[.a{fill:red}]]></style>";
        assert_eq!(process(svg(body).as_bytes()), Err(IconError::UnsafeSvg));
    }

    #[test]
    fn animation_elements_are_rejected() {
        for body in [
            r#"<path d="M0 0"><animate attributeName="href" to="javascript:x"/></path>"#,
            r#"<path d="M0 0"><set attributeName="fill" to="red"/></path>"#,
        ] {
            assert_eq!(
                process(svg(body).as_bytes()),
                Err(IconError::UnsafeSvg),
                "{body}"
            );
        }
    }

    #[test]
    fn a_foreign_namespace_is_rejected() {
        let raw = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:evil="http://attacker.example/ns" viewBox="0 0 1 1"><path d="M0 0"/></svg>"#;
        assert_eq!(process(raw.as_bytes()), Err(IconError::UnsafeSvg));
    }

    #[test]
    fn editor_metadata_is_dropped() {
        let body = "<metadata><rdf>lots of editor cruft</rdf></metadata><title>My Logo</title><desc>drawn in something</desc><path d=\"M0 0\"/>";
        let out = process(svg(body).as_bytes()).expect("accepted");
        let text = String::from_utf8(out.icon.bytes).expect("utf8");
        assert!(!text.contains("metadata"), "{text}");
        assert!(!text.contains("My Logo"), "{text}");
        assert!(!text.contains("editor cruft"), "{text}");
        assert!(text.contains("<path"));
    }

    #[test]
    fn comments_are_dropped() {
        let out = process(svg("<!-- generated by something --><path d=\"M0 0\"/>").as_bytes())
            .expect("accepted");
        let text = String::from_utf8(out.icon.bytes).expect("utf8");
        assert!(!text.contains("generated by"), "{text}");
    }

    #[test]
    fn an_svg_over_the_stored_ceiling_is_refused() {
        // Well-formed, allowlisted, and far too big to keep.
        let body = format!(r#"<path d="{}"/>"#, "M0 0".repeat(20_000));
        assert_eq!(
            process(svg(&body).as_bytes()),
            Err(IconError::StillTooLarge)
        );
    }

    #[test]
    fn no_error_message_can_carry_file_contents() {
        // Every variant is a unit variant, so there is nowhere for a fragment of the
        // upload to hide. This is the type-level version of CLAUDE.md §4.6.
        for e in [
            IconError::TooLarge,
            IconError::Unsupported,
            IconError::Implausible,
            IconError::UnsafeSvg,
            IconError::StillTooLarge,
            IconError::Encode,
        ] {
            let rendered = e.to_string();
            assert!(!rendered.is_empty());
            assert!(!rendered.contains("attacker"));
        }
    }
}
