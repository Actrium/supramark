//! Shared constants for the deflate compressor.
//!
//! Mirror Go's `compress/flate` constants exactly. Renamed to Rust SCREAMING
//! _SNAKE casing but the underlying values are unchanged.

// ---------------------------------------------------------------------------
// LZ77 window / match constants (deflate.go)
// ---------------------------------------------------------------------------
pub(crate) const LOG_WINDOW_SIZE: u32 = 15;
pub(crate) const WINDOW_SIZE: usize = 1 << LOG_WINDOW_SIZE;
pub(crate) const WINDOW_MASK: usize = WINDOW_SIZE - 1;

pub(crate) const BASE_MATCH_LENGTH: usize = 3;
pub(crate) const MIN_MATCH_LENGTH: usize = 4;
pub(crate) const MAX_MATCH_LENGTH: usize = 258;
pub(crate) const BASE_MATCH_OFFSET: usize = 1;
#[allow(dead_code)]
pub(crate) const MAX_MATCH_OFFSET: usize = 1 << 15;

pub(crate) const MAX_FLATE_BLOCK_TOKENS: usize = 1 << 14;
pub(crate) const MAX_STORE_BLOCK_SIZE: usize = 65535;
pub(crate) const HASH_BITS: u32 = 17;
pub(crate) const HASH_SIZE: usize = 1 << HASH_BITS;
pub(crate) const HASH_MASK: u32 = (1 << HASH_BITS) - 1;
pub(crate) const MAX_HASH_OFFSET: i32 = 1 << 24;

pub(crate) const SKIP_NEVER: i32 = i32::MAX;

// ---------------------------------------------------------------------------
// Huffman / block constants (huffman_bit_writer.go)
// ---------------------------------------------------------------------------
pub(crate) const MAX_NUM_LIT: usize = 286;
pub(crate) const OFFSET_CODE_COUNT: usize = 30;
pub(crate) const END_BLOCK_MARKER: usize = 256;
pub(crate) const LENGTH_CODES_START: usize = 257;
pub(crate) const CODEGEN_CODE_COUNT: usize = 19;
pub(crate) const BAD_CODE: u8 = 255;

pub(crate) const BUFFER_FLUSH_SIZE: usize = 240;
pub(crate) const BUFFER_SIZE: usize = BUFFER_FLUSH_SIZE + 8;
