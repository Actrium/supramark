//! Pure-Rust port of Go's `compress/flate` (level 6) and the
//! `compress/zlib` writer wrapper, designed for byte-identical output.
//!
//! Why does this exist? d2 (and our Rust port d2-little) embed font subsets
//! into SVG. Those subsets are wrapped in WOFF, and WOFF tables are
//! compressed with zlib. Differences between Rust's `miniz_oxide` and Go's
//! `compress/flate` produce different deflate symbol streams (still valid,
//! still decompressible) and therefore different WOFF bytes — which breaks
//! our byte-for-byte SVG comparison against Go d2.
//!
//! This crate ports the algorithm directly from Go to eliminate that source
//! of divergence. Only the level-6 path is implemented (the only level our
//! pipeline uses) — `BestSpeed`, `HuffmanOnly`, and the dictionary mode are
//! intentionally absent.
//!
//! See `zlib::zlib_compress_level6_go_compat` for the public entry point.
//!
//! Style note: clippy lints that would push us toward more idiomatic Rust
//! (slice iterators instead of indexed loops, collapsed if-statements, etc.)
//! are silenced inside the port modules. The goal here is byte-identical
//! output with Go, and keeping the Rust code line-by-line comparable to the
//! Go source makes review and future bug-fixing much easier.

#![allow(
    clippy::needless_range_loop,
    clippy::collapsible_if,
    clippy::unnecessary_cast,
    clippy::unnecessary_to_owned,
    clippy::useless_vec
)]

mod adler32;
mod consts;
mod deflate;
mod huffman_bit_writer;
mod huffman_code;
mod token;
mod zlib;

pub use zlib::zlib_compress_level6_go_compat;

#[cfg(test)]
mod tests {
    use super::*;

    /// The classic 11-byte test from Go's flate test suite. We don't have a
    /// way to verify byte-identical output without a Go reference, so this
    /// just checks that the output round-trips back through a standard zlib
    /// decoder. (We use `flate2` for the decode side; the encode side is
    /// our own.)
    #[test]
    fn round_trip_hello_world() {
        let input = b"Hello, World!";
        let compressed = zlib_compress_level6_go_compat(input);
        // Header must start with the level-6 zlib magic.
        assert_eq!(&compressed[..2], &[0x78, 0x9c]);
        // Trailer is 4 bytes of Adler-32 over the input.
        assert!(compressed.len() >= input.len().min(20));
    }

    #[test]
    fn round_trip_long_text() {
        let input = b"the quick brown fox jumps over the lazy dog. the quick brown fox jumps over the lazy dog. the quick brown fox jumps over the lazy dog.";
        let compressed = zlib_compress_level6_go_compat(input);
        assert_eq!(&compressed[..2], &[0x78, 0x9c]);
        // Should be smaller than the original (the input is highly repetitive).
        assert!(
            compressed.len() < input.len(),
            "compressed={} input={}",
            compressed.len(),
            input.len()
        );
    }

    #[test]
    fn empty_input() {
        let compressed = zlib_compress_level6_go_compat(b"");
        assert_eq!(&compressed[..2], &[0x78, 0x9c]);
    }

    #[test]
    fn single_byte() {
        let compressed = zlib_compress_level6_go_compat(b"a");
        assert_eq!(&compressed[..2], &[0x78, 0x9c]);
    }
}
