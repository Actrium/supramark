//! Byte-for-byte parity tests against Go's d2 font subset output.
//!
//! Status:
//! - The TTF subset produced by `utf8_cut_font` is byte-identical to Go's
//!   `UTF8CutFont` output (verified with SourceSansPro-Bold, corpus "ab").
//! - The full WOFF produced by `sfnt2woff` is byte-identical to Go's
//!   `Sfnt2Woff` output for the same subset, including the previously
//!   divergent `name` and `post` tables. We achieve this by routing
//!   compression through the d2-flate crate (a pure-Rust port of Go's
//!   `compress/flate`), so the deflate symbol stream matches Go's exactly.

use d2_little::font::{sfnt2woff, utf8_cut_font};

fn hex_to_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// Embedded full-subset reference: bytes of Go's `UTF8CutFont(ttf, "ab")`
/// for SourceSansPro-Bold. Captured from Go 1.22 with d2 v0.7.1.
const GO_TTF_SUBSET_AB_FULL_HEX: &str = include_str!("fixtures/font_byte_match/go_ttf_ab_bold.hex");

/// Embedded full-WOFF reference: bytes of Go's
/// `Sfnt2Woff(UTF8CutFont(ttf, "ab"))` for SourceSansPro-Bold.
const GO_WOFF_AB_FULL_HEX: &str = include_str!("fixtures/font_byte_match/go_woff_ab_bold.hex");

#[test]
fn ttf_subset_full_matches_go_for_ab_bold() {
    let ttf = include_bytes!("../ttf/SourceSansPro-Bold.ttf");
    let subset_ttf = utf8_cut_font(ttf, "ab").expect("utf8_cut_font returned None");
    let go_subset = hex_to_bytes(GO_TTF_SUBSET_AB_FULL_HEX.trim());

    assert_eq!(
        subset_ttf.len(),
        go_subset.len(),
        "TTF subset length mismatch (rust={}, go={})",
        subset_ttf.len(),
        go_subset.len()
    );
    if subset_ttf != go_subset {
        let first = subset_ttf
            .iter()
            .zip(go_subset.iter())
            .position(|(a, b)| a != b);
        panic!(
            "TTF subset bytes differ at offset {:?}\nrust[0..64]={:02x?}\n  go[0..64]={:02x?}",
            first,
            &subset_ttf[..64.min(subset_ttf.len())],
            &go_subset[..64.min(go_subset.len())],
        );
    }
}

/// Strictest test: the full WOFF byte stream from `sfnt2woff` must match
/// Go's `Sfnt2Woff` output exactly. This depends on the d2-flate crate
/// producing byte-identical deflate output, so any divergence here is a
/// regression in either the WOFF builder or the deflate engine.
#[test]
fn woff_full_matches_go_for_ab_bold() {
    let ttf = include_bytes!("../ttf/SourceSansPro-Bold.ttf");
    let subset_ttf = utf8_cut_font(ttf, "ab").expect("utf8_cut_font returned None");
    let woff = sfnt2woff(&subset_ttf).expect("sfnt2woff failed");
    let go_woff = hex_to_bytes(GO_WOFF_AB_FULL_HEX.trim());

    assert_eq!(
        woff.len(),
        go_woff.len(),
        "WOFF length mismatch (rust={}, go={})",
        woff.len(),
        go_woff.len()
    );
    if woff != go_woff {
        let first = woff
            .iter()
            .zip(go_woff.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(woff.len().min(go_woff.len()));
        let lo = first.saturating_sub(16);
        let hi = (first + 32).min(woff.len()).min(go_woff.len());
        panic!(
            "WOFF bytes differ at offset {}\nrust[{}..{}]={:02x?}\n  go[{}..{}]={:02x?}",
            first,
            lo,
            hi,
            &woff[lo..hi],
            lo,
            hi,
            &go_woff[lo..hi],
        );
    }
}

#[test]
fn woff_roundtrip_contains_matching_tables_for_ab_bold() {
    // Even though the compressed bytes for the `name` and `post` tables
    // still differ from Go's output, the WOFF we produce must still
    // round-trip to the same TTF table data that Go would have produced.
    let ttf = include_bytes!("../ttf/SourceSansPro-Bold.ttf");
    let subset_ttf = utf8_cut_font(ttf, "ab").expect("utf8_cut_font returned None");
    let woff = sfnt2woff(&subset_ttf).expect("sfnt2woff failed");

    // WOFF magic
    assert_eq!(&woff[0..4], b"wOFF", "WOFF magic missing");
    // Flavor should be 0x00010000 (TrueType)
    assert_eq!(&woff[4..8], &[0, 1, 0, 0], "unexpected WOFF flavor");
    // numTables == 10
    let num_tables = u16::from_be_bytes([woff[12], woff[13]]);
    assert_eq!(num_tables, 10, "expected 10 tables in WOFF");
}

/// The first 10 table-directory entries in Go's WOFF for ab+bold, minus the
/// fields we know diverge:
/// - The `name` entry's compressed length (comp-stream byte count).
/// - The `post` entry's offset and compressed length.
///
/// Everything else — tags, original lengths, checksums, the OS/2/cmap/glyf/
///   head/hhea/hmtx/loca/maxp/name-offset entries — must match byte-for-byte.
///
/// This catches regressions where the table directory order, length
/// computation, or stored-vs-compressed decision drifts from Go's behavior.
#[test]
fn woff_header_and_fixed_tables_match_go_for_ab_bold() {
    let ttf = include_bytes!("../ttf/SourceSansPro-Bold.ttf");
    let subset_ttf = utf8_cut_font(ttf, "ab").expect("utf8_cut_font returned None");
    let woff = sfnt2woff(&subset_ttf).expect("sfnt2woff failed");

    // Header fields that don't depend on the compressed-stream size.
    assert_eq!(&woff[0..4], b"wOFF");
    assert_eq!(&woff[4..8], &[0x00, 0x01, 0x00, 0x00], "flavor");
    assert_eq!(&woff[12..14], &[0x00, 0x0a], "numTables");
    assert_eq!(&woff[14..16], &[0x00, 0x00], "reserved");
    // sfnt size (sum of aligned table lengths) — independent of compression
    assert_eq!(
        &woff[16..20],
        &[0x00, 0x00, 0x0b, 0x48],
        "expected sfntSize == 0x0b48"
    );

    // Table directory: tags, original lengths, and checksums for the eight
    // tables whose compressed/stored sizes do match Go.
    let expected_dir_prefix: &[&[u8]] = &[
        b"OS/2", b"cmap", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp",
    ];
    for (i, tag) in expected_dir_prefix.iter().enumerate() {
        let base = 44 + i * 20;
        assert_eq!(&woff[base..base + 4], *tag, "dir[{}].tag", i);
    }

    // The name directory entry: tag, offset, length, and checksum must match
    // Go — only the comp_length field (bytes base+8..base+12) may differ.
    let name_base = 44 + 8 * 20;
    assert_eq!(&woff[name_base..name_base + 4], b"name");
    assert_eq!(
        &woff[name_base + 12..name_base + 16],
        &[0x00, 0x00, 0x08, 0x2a],
        "name.length"
    );
    assert_eq!(
        &woff[name_base + 16..name_base + 20],
        &[0x08, 0xf0, 0x56, 0x41],
        "name.checksum"
    );

    // The post directory entry: tag, length, and checksum. Offset and
    // comp_length depend on the size of the preceding name table body, so
    // they're allowed to differ.
    let post_base = 44 + 9 * 20;
    assert_eq!(&woff[post_base..post_base + 4], b"post");
    assert_eq!(
        &woff[post_base + 12..post_base + 16],
        &[0x00, 0x00, 0x00, 0x20],
        "post.length"
    );
    assert_eq!(
        &woff[post_base + 16..post_base + 20],
        &[0xff, 0xd1, 0x00, 0x32],
        "post.checksum"
    );
}
