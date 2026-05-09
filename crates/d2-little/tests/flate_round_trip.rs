//! Round-trip tests: compress with our pure-Rust implementation, decompress
//! with `flate2` (which delegates to `miniz_oxide`). If our zlib stream is
//! malformed, this will panic immediately.
//!
//! These tests are NOT byte-identical checks against Go — that comparison
//! happens in the d2-font crate, where we have known-good Go fixture data.

use std::io::Read;

use d2_little::flate::zlib_compress_level6_go_compat;
use flate2::read::ZlibDecoder;

fn round_trip(input: &[u8]) {
    let compressed = zlib_compress_level6_go_compat(input);
    let mut decoder = ZlibDecoder::new(&compressed[..]);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .expect("decompression failed");
    assert_eq!(decompressed, input, "round-trip mismatch");
}

#[test]
fn empty() {
    round_trip(b"");
}

#[test]
fn small_literal() {
    round_trip(b"a");
    round_trip(b"abc");
    round_trip(b"hello world");
}

#[test]
fn repetitive_short() {
    // Likely to trigger LZ77 matches.
    round_trip(b"aaaaaaaaaaaaaaaaaaaa");
    round_trip(b"abcabcabcabcabcabcabc");
}

#[test]
fn repetitive_long() {
    let mut buf = Vec::new();
    for _ in 0..1000 {
        buf.extend_from_slice(b"the quick brown fox jumps over the lazy dog. ");
    }
    round_trip(&buf);
}

#[test]
fn binary_random_like() {
    // Pseudorandom data — should be incompressible-ish, so the compressor
    // should fall back to stored blocks for some chunks.
    let mut buf = Vec::with_capacity(8192);
    let mut state: u32 = 0xdead_beef;
    for _ in 0..8192 {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        buf.push((state >> 16) as u8);
    }
    round_trip(&buf);
}

#[test]
fn larger_than_window() {
    // Force the window-shift path in fillDeflate.
    let mut buf = Vec::with_capacity(100_000);
    for i in 0..100_000 {
        buf.push((i & 0xFF) as u8);
    }
    round_trip(&buf);
}

#[test]
fn font_table_sized() {
    // Realistic font-table inputs: a few hundred bytes to a few KB.
    let mut buf = Vec::new();
    for i in 0..2000 {
        buf.push((i * 7 + (i / 13) * 3) as u8);
    }
    round_trip(&buf);
    // Highly structured (mimicking glyph data).
    let buf2 = vec![0u8; 4096];
    round_trip(&buf2);
}
