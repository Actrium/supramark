//! LZ77 compressor: hash chains, lazy match selection, block emission.
//!
//! Faithful port of Go's `compress/flate/deflate.go`. Only the levels-2-to-9
//! "general" path is ported (the only path our font pipeline uses; the
//! BestSpeed and HuffmanOnly fast paths are intentionally omitted).
//!
//! Algorithm correctness is critical for byte-identical output:
//!
//! * `hash4` and `bulk_hash4` use the exact `hashmul = 0x1e35a7bd` constant
//!   and `>> (32 - hashBits)` shift Go uses.
//! * `find_match` walks the hash chain in the same order, applies the same
//!   `tries >>= 2` "good enough" shortcut, and the same `pos-i <= 4096` cutoff.
//! * The lazy-match selection in `deflate` mirrors Go's somewhat tortured
//!   condition with `prev_length`, `byte_available`, and `fast_skip_hashing`.

use crate::flate::consts::{
    BASE_MATCH_LENGTH, BASE_MATCH_OFFSET, HASH_BITS, HASH_MASK, HASH_SIZE, MAX_FLATE_BLOCK_TOKENS,
    MAX_HASH_OFFSET, MAX_MATCH_LENGTH, MIN_MATCH_LENGTH, SKIP_NEVER, WINDOW_MASK, WINDOW_SIZE,
};
use crate::flate::huffman_bit_writer::HuffmanBitWriter;
use crate::flate::token::Token;

#[derive(Copy, Clone, Debug)]
struct CompressionLevel {
    #[allow(dead_code)]
    level: i32,
    good: i32,
    lazy: i32,
    nice: i32,
    chain: i32,
    fast_skip_hashing: i32,
}

/// `levels[6]` is what zlib's default compression uses, which matches what
/// Go's `compress/zlib` calls when no level is supplied.
static LEVELS: [CompressionLevel; 10] = [
    CompressionLevel {
        level: 0,
        good: 0,
        lazy: 0,
        nice: 0,
        chain: 0,
        fast_skip_hashing: 0,
    },
    CompressionLevel {
        level: 1,
        good: 0,
        lazy: 0,
        nice: 0,
        chain: 0,
        fast_skip_hashing: 0,
    },
    CompressionLevel {
        level: 2,
        good: 4,
        lazy: 0,
        nice: 16,
        chain: 8,
        fast_skip_hashing: 5,
    },
    CompressionLevel {
        level: 3,
        good: 4,
        lazy: 0,
        nice: 32,
        chain: 32,
        fast_skip_hashing: 6,
    },
    CompressionLevel {
        level: 4,
        good: 4,
        lazy: 4,
        nice: 16,
        chain: 16,
        fast_skip_hashing: SKIP_NEVER,
    },
    CompressionLevel {
        level: 5,
        good: 8,
        lazy: 16,
        nice: 32,
        chain: 32,
        fast_skip_hashing: SKIP_NEVER,
    },
    CompressionLevel {
        level: 6,
        good: 8,
        lazy: 16,
        nice: 128,
        chain: 128,
        fast_skip_hashing: SKIP_NEVER,
    },
    CompressionLevel {
        level: 7,
        good: 8,
        lazy: 32,
        nice: 128,
        chain: 256,
        fast_skip_hashing: SKIP_NEVER,
    },
    CompressionLevel {
        level: 8,
        good: 32,
        lazy: 128,
        nice: 258,
        chain: 1024,
        fast_skip_hashing: SKIP_NEVER,
    },
    CompressionLevel {
        level: 9,
        good: 32,
        lazy: 258,
        nice: 258,
        chain: 4096,
        fast_skip_hashing: SKIP_NEVER,
    },
];

const HASHMUL: u32 = 0x1e35a7bd;

/// Hash four bytes the same way Go's `hash4` does. Caller must ensure
/// `b.len() >= 4`.
#[inline]
fn hash4(b: &[u8]) -> u32 {
    let v = (b[3] as u32) | ((b[2] as u32) << 8) | ((b[1] as u32) << 16) | ((b[0] as u32) << 24);
    v.wrapping_mul(HASHMUL) >> (32 - HASH_BITS)
}

// `bulkHash4` from Go's deflate is only used by `fillWindow` (preset
// dictionaries). Our zlib path doesn't use a dictionary, so we omit it.

/// Number of matching bytes between `a` and `b`, capped at `max`. Both
/// slices must be at least `max` bytes long.
#[inline]
fn match_len(a: &[u8], b: &[u8], max: usize) -> usize {
    let a = &a[..max];
    let b = &b[..a.len()];
    for i in 0..a.len() {
        if b[i] != a[i] {
            return i;
        }
    }
    max
}

pub(crate) struct Compressor {
    level: CompressionLevel,

    pub(crate) w: HuffmanBitWriter,

    // Hash chains
    chain_head: i32,
    hash_head: Vec<u32>, // length HASH_SIZE
    hash_prev: Vec<u32>, // length WINDOW_SIZE
    hash_offset: i32,

    // Input window
    index: i32,
    window: Vec<u8>,
    window_end: i32,
    block_start: i32,
    byte_available: bool,

    sync: bool,

    // Queued output tokens for the current block
    pub(crate) tokens: Vec<Token>,

    // Lazy-match state carried between iterations of `deflate`
    length: i32,
    offset: i32,
    max_insert_index: i32,
}

impl Compressor {
    /// Create a level-6 compressor (the default zlib level). Other levels are
    /// not supported by this port — d2-font only ever calls level 6.
    pub(crate) fn new_level6() -> Self {
        let level = LEVELS[6];
        let mut c = Self {
            level,
            w: HuffmanBitWriter::new(),
            chain_head: -1,
            hash_head: vec![0; HASH_SIZE],
            hash_prev: vec![0; WINDOW_SIZE],
            hash_offset: 1,
            index: 0,
            window: vec![0u8; 2 * WINDOW_SIZE],
            window_end: 0,
            block_start: 0,
            byte_available: false,
            sync: false,
            tokens: Vec::with_capacity(MAX_FLATE_BLOCK_TOKENS + 1),
            length: (MIN_MATCH_LENGTH - 1) as i32,
            offset: 0,
            max_insert_index: 0,
        };
        c.tokens.clear();
        c
    }

    /// Direct port of `fillDeflate`. Returns the number of bytes copied.
    fn fill_deflate(&mut self, b: &[u8]) -> usize {
        if self.index >= (2 * WINDOW_SIZE - (MIN_MATCH_LENGTH + MAX_MATCH_LENGTH)) as i32 {
            // Shift the window down by WINDOW_SIZE.
            self.window.copy_within(WINDOW_SIZE..2 * WINDOW_SIZE, 0);
            self.index -= WINDOW_SIZE as i32;
            self.window_end -= WINDOW_SIZE as i32;
            if self.block_start >= WINDOW_SIZE as i32 {
                self.block_start -= WINDOW_SIZE as i32;
            } else {
                self.block_start = i32::MAX;
            }
            self.hash_offset += WINDOW_SIZE as i32;
            if self.hash_offset > MAX_HASH_OFFSET {
                let delta = self.hash_offset - 1;
                self.hash_offset -= delta;
                self.chain_head -= delta;

                // Subtract `delta` from every hash entry, clamping to 0.
                for v in self.hash_prev.iter_mut() {
                    if (*v as i32) > delta {
                        *v = (*v as i32 - delta) as u32;
                    } else {
                        *v = 0;
                    }
                }
                for v in self.hash_head.iter_mut() {
                    if (*v as i32) > delta {
                        *v = (*v as i32 - delta) as u32;
                    } else {
                        *v = 0;
                    }
                }
            }
        }
        let dst_start = self.window_end as usize;
        let n = (self.window.len() - dst_start).min(b.len());
        self.window[dst_start..dst_start + n].copy_from_slice(&b[..n]);
        self.window_end += n as i32;
        n
    }

    fn write_block(&mut self, index: i32) {
        if index > 0 {
            let window_slice: Option<Vec<u8>> = if self.block_start <= index {
                Some(self.window[self.block_start as usize..index as usize].to_vec())
            } else {
                None
            };
            self.block_start = index;
            // Take tokens out so we can pass them mutably to the writer.
            let mut tokens = std::mem::take(&mut self.tokens);
            self.w
                .write_block(&mut tokens, false, window_slice.as_deref());
            tokens.clear();
            self.tokens = tokens;
        }
    }

    /// Try to find a match starting at `pos`. Direct port of `findMatch`.
    /// Returns `(length, offset, ok)`.
    fn find_match(
        &self,
        pos: i32,
        prev_head: i32,
        prev_length: i32,
        lookahead: i32,
    ) -> (i32, i32, bool) {
        let mut min_match_look = MAX_MATCH_LENGTH as i32;
        if lookahead < min_match_look {
            min_match_look = lookahead;
        }

        // win[..pos+min_match_look]
        let win_end = (pos + min_match_look) as usize;
        let win = &self.window[..win_end];

        // Quit early once we find a match this long.
        let mut nice = (win.len() as i32) - pos;
        if (self.level.nice as i32) < nice {
            nice = self.level.nice as i32;
        }

        let mut tries = self.level.chain;
        let mut length = prev_length;
        if length >= self.level.good {
            tries >>= 2;
        }

        let mut w_end = win[(pos + length) as usize];
        let w_pos = &win[pos as usize..];
        let min_index = pos - WINDOW_SIZE as i32;

        let mut offset_out: i32 = 0;
        let mut ok = false;

        let mut i = prev_head;
        while tries > 0 {
            if w_end == win[(i + length) as usize] {
                let n = match_len(&win[i as usize..], w_pos, min_match_look as usize) as i32;

                if n > length && (n > MIN_MATCH_LENGTH as i32 || pos - i <= 4096) {
                    length = n;
                    offset_out = pos - i;
                    ok = true;
                    if n >= nice {
                        break;
                    }
                    w_end = win[(pos + n) as usize];
                }
            }
            if i == min_index {
                // hashPrev[i & windowMask] has been overwritten, stop now.
                break;
            }
            i = (self.hash_prev[(i as usize) & WINDOW_MASK] as i32) - self.hash_offset;
            if i < min_index || i < 0 {
                break;
            }
            tries -= 1;
        }
        (length, offset_out, ok)
    }

    /// Direct port of `deflate`. The main LZ77 loop with lazy matching.
    fn deflate(&mut self) {
        if (self.window_end - self.index) < (MIN_MATCH_LENGTH + MAX_MATCH_LENGTH) as i32
            && !self.sync
        {
            return;
        }

        self.max_insert_index = self.window_end - (MIN_MATCH_LENGTH - 1) as i32;

        loop {
            if self.index > self.window_end {
                panic!("index > windowEnd");
            }
            let lookahead = self.window_end - self.index;
            if lookahead < (MIN_MATCH_LENGTH + MAX_MATCH_LENGTH) as i32 {
                if !self.sync {
                    break;
                }
                if self.index > self.window_end {
                    panic!("index > windowEnd");
                }
                if lookahead == 0 {
                    // Flush current block.
                    if self.byte_available {
                        let lit = self.window[(self.index - 1) as usize] as u32;
                        self.tokens.push(Token::literal(lit));
                        self.byte_available = false;
                    }
                    if !self.tokens.is_empty() {
                        self.write_block(self.index);
                    }
                    break;
                }
            }

            if self.index < self.max_insert_index {
                let h = hash4(
                    &self.window[self.index as usize..self.index as usize + MIN_MATCH_LENGTH],
                );
                let hh_idx = (h & HASH_MASK) as usize;
                let hh = self.hash_head[hh_idx];
                self.chain_head = hh as i32;
                self.hash_prev[(self.index as usize) & WINDOW_MASK] = self.chain_head as u32;
                self.hash_head[hh_idx] = (self.index + self.hash_offset) as u32;
            }

            let prev_length = self.length;
            let prev_offset = self.offset;
            self.length = (MIN_MATCH_LENGTH - 1) as i32;
            self.offset = 0;
            let mut min_index = self.index - WINDOW_SIZE as i32;
            if min_index < 0 {
                min_index = 0;
            }

            // Decide whether to look for a match at this position.
            let try_match = self.chain_head - self.hash_offset >= min_index
                && ((self.level.fast_skip_hashing != SKIP_NEVER
                    && lookahead > MIN_MATCH_LENGTH as i32 - 1)
                    || (self.level.fast_skip_hashing == SKIP_NEVER
                        && lookahead > prev_length
                        && prev_length < self.level.lazy));

            if try_match {
                let (new_length, new_offset, ok) = self.find_match(
                    self.index,
                    self.chain_head - self.hash_offset,
                    (MIN_MATCH_LENGTH - 1) as i32,
                    lookahead,
                );
                if ok {
                    self.length = new_length;
                    self.offset = new_offset;
                }
            }

            // Choose between emitting a match or a literal (lazy logic).
            let emit_match = (self.level.fast_skip_hashing != SKIP_NEVER
                && self.length >= MIN_MATCH_LENGTH as i32)
                || (self.level.fast_skip_hashing == SKIP_NEVER
                    && prev_length >= MIN_MATCH_LENGTH as i32
                    && self.length <= prev_length);

            if emit_match {
                if self.level.fast_skip_hashing != SKIP_NEVER {
                    self.tokens.push(Token::match_token(
                        (self.length - BASE_MATCH_LENGTH as i32) as u32,
                        (self.offset - BASE_MATCH_OFFSET as i32) as u32,
                    ));
                } else {
                    self.tokens.push(Token::match_token(
                        (prev_length - BASE_MATCH_LENGTH as i32) as u32,
                        (prev_offset - BASE_MATCH_OFFSET as i32) as u32,
                    ));
                }

                if self.length <= self.level.fast_skip_hashing {
                    let new_index = if self.level.fast_skip_hashing != SKIP_NEVER {
                        self.index + self.length
                    } else {
                        self.index + prev_length - 1
                    };
                    let mut index = self.index;
                    index += 1;
                    while index < new_index {
                        if index < self.max_insert_index {
                            let h = hash4(
                                &self.window[index as usize..index as usize + MIN_MATCH_LENGTH],
                            );
                            let hh_idx = (h & HASH_MASK) as usize;
                            let hh = self.hash_head[hh_idx];
                            self.hash_prev[(index as usize) & WINDOW_MASK] = hh;
                            self.hash_head[hh_idx] = (index + self.hash_offset) as u32;
                        }
                        index += 1;
                    }
                    self.index = index;

                    if self.level.fast_skip_hashing == SKIP_NEVER {
                        self.byte_available = false;
                        self.length = (MIN_MATCH_LENGTH - 1) as i32;
                    }
                } else {
                    // Long matches: don't bother inserting each item.
                    self.index += self.length;
                }
                if self.tokens.len() == MAX_FLATE_BLOCK_TOKENS {
                    self.write_block(self.index);
                }
            } else {
                if self.level.fast_skip_hashing != SKIP_NEVER || self.byte_available {
                    let i = if self.level.fast_skip_hashing != SKIP_NEVER {
                        self.index
                    } else {
                        self.index - 1
                    };
                    self.tokens
                        .push(Token::literal(self.window[i as usize] as u32));
                    if self.tokens.len() == MAX_FLATE_BLOCK_TOKENS {
                        self.write_block(i + 1);
                    }
                }
                self.index += 1;
                if self.level.fast_skip_hashing == SKIP_NEVER {
                    self.byte_available = true;
                }
            }
        }
    }

    /// Direct port of the level-6 path of `compressor.write`.
    pub(crate) fn write(&mut self, mut b: &[u8]) {
        while !b.is_empty() {
            self.deflate();
            let n = self.fill_deflate(b);
            b = &b[n..];
        }
    }

    /// Direct port of `syncFlush`. Emits an empty stored block as a sync marker.
    pub(crate) fn sync_flush(&mut self) {
        self.sync = true;
        self.deflate();
        self.w.write_stored_header(0, false);
        self.w.flush();
        self.sync = false;
    }

    /// Direct port of `close`. Emits a final empty stored block.
    pub(crate) fn close(&mut self) {
        self.sync = true;
        self.deflate();
        self.w.write_stored_header(0, true);
        self.w.flush();
    }
}
