//! Huffman code generation for deflate blocks.
//!
//! Faithful port of Go's `compress/flate/huffman_code.go`. Producing
//! byte-identical output requires matching every detail:
//!
//! * `bitCounts` uses Go's exact iterative chain-building algorithm.
//! * The two sort orders (`byLiteral`, `byFreq`) are stable; we use Rust's
//!   `sort_by` which is also stable, plus the same comparison rules.
//! * Code assignment walks `bitCount` from the highest bit length downward,
//!   with `code` shifted left at each step (matching Go).
//! * `reverse_bits` uses the same `(number << (16 - bitLength)).reverse_bits()`
//!   formula so the bit-reversed code values match Go bit-for-bit.

use crate::flate::consts::MAX_NUM_LIT;

#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct HCode {
    pub(crate) code: u16,
    pub(crate) len: u16,
}

impl HCode {
    #[inline]
    pub(crate) fn set(&mut self, code: u16, length: u16) {
        self.len = length;
        self.code = code;
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct LiteralNode {
    pub(crate) literal: u16,
    pub(crate) freq: i32,
}

const fn max_node() -> LiteralNode {
    LiteralNode {
        literal: u16::MAX,
        freq: i32::MAX,
    }
}

/// Tracks chain-building state for one bit-length level inside `bit_counts`.
#[derive(Copy, Clone, Debug, Default)]
struct LevelInfo {
    level: i32,
    last_freq: i32,
    next_char_freq: i32,
    next_pair_freq: i32,
    needed: i32,
}

pub(crate) struct HuffmanEncoder {
    pub(crate) codes: Vec<HCode>,
    /// Reusable buffer for `generate`. We allocate it lazily on first use to
    /// match Go's behavior (Go also lazily allocates `freqcache`).
    freq_cache: Option<Vec<LiteralNode>>,
    bit_count: [i32; 17],
}

const MAX_BITS_LIMIT: i32 = 16;

impl HuffmanEncoder {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            codes: vec![HCode::default(); size],
            freq_cache: None,
            bit_count: [0; 17],
        }
    }

    /// Sum over all non-zero frequencies of `freq[i] * codes[i].len`.
    /// Used to compare encoding sizes when picking block type.
    pub(crate) fn bit_length(&self, freq: &[i32]) -> usize {
        let mut total = 0usize;
        for (i, &f) in freq.iter().enumerate() {
            if f != 0 {
                total += (f as usize) * (self.codes[i].len as usize);
            }
        }
        total
    }

    /// Compute the number of literals that should be encoded with each bit
    /// length. Direct port of Go's `bitCounts`. Returns a slice of length
    /// `max_bits + 1` (index 0 is unused, matching Go).
    fn bit_counts(&mut self, list: &mut Vec<LiteralNode>, mut max_bits: i32) -> &[i32] {
        if max_bits >= MAX_BITS_LIMIT {
            panic!("flate: maxBits too large");
        }
        let n = list.len() as i32;
        // Append the sentinel max-node so the iteration below can read past
        // the real data without bounds-checking.
        list.push(max_node());

        if max_bits > n - 1 {
            max_bits = n - 1;
        }

        // levels[0] is a bogus level whose only role is making
        // `levels[1].prev.needed == 0`.
        let mut levels: [LevelInfo; MAX_BITS_LIMIT as usize] = [LevelInfo::default(); 16];
        // leaf_counts[i][j]: number of literals at the left of the level-j
        // ancestor of the rightmost node at level i.
        let mut leaf_counts: [[i32; MAX_BITS_LIMIT as usize]; MAX_BITS_LIMIT as usize] =
            [[0; 16]; 16];

        for level in 1..=max_bits {
            let lv = level as usize;
            levels[lv] = LevelInfo {
                level,
                last_freq: list[1].freq,
                next_char_freq: list[2].freq,
                next_pair_freq: list[0].freq + list[1].freq,
                needed: 0,
            };
            leaf_counts[lv][lv] = 2;
            if level == 1 {
                levels[lv].next_pair_freq = i32::MAX;
            }
        }

        // We need a total of 2*n - 2 items at the top level and have already
        // generated 2.
        levels[max_bits as usize].needed = 2 * n - 4;

        let mut level = max_bits;
        loop {
            let lv = level as usize;
            // Borrow checker dance: take a copy of the level state, mutate
            // locally, then write back at the end of each iteration.
            let mut l = levels[lv];

            if l.next_pair_freq == i32::MAX && l.next_char_freq == i32::MAX {
                // Out of leaves and pairs at this level — finalize and step up.
                l.needed = 0;
                levels[lv] = l;
                levels[lv + 1].next_pair_freq = i32::MAX;
                level += 1;
                continue;
            }

            let prev_freq = l.last_freq;
            if l.next_char_freq < l.next_pair_freq {
                // The next item on this row is a leaf node.
                let nl = leaf_counts[lv][lv] + 1;
                l.last_freq = l.next_char_freq;
                leaf_counts[lv][lv] = nl;
                l.next_char_freq = list[nl as usize].freq;
            } else {
                // The next item on this row is a pair from the previous row.
                l.last_freq = l.next_pair_freq;
                // Copy lower-level leaf counts (except `counts[level]` which
                // remains the same).
                for j in 0..lv {
                    leaf_counts[lv][j] = leaf_counts[lv - 1][j];
                }
                levels[(l.level - 1) as usize].needed = 2;
            }

            l.needed -= 1;
            if l.needed == 0 {
                // Done with this level. Bubble up the synthetic pair to the
                // parent and continue.
                levels[lv] = l;
                if l.level == max_bits {
                    break;
                }
                levels[(l.level + 1) as usize].next_pair_freq = prev_freq + l.last_freq;
                level += 1;
            } else {
                levels[lv] = l;
                // If we stole from below, descend temporarily to replenish.
                while levels[(level - 1) as usize].needed > 0 {
                    level -= 1;
                }
            }
        }

        if leaf_counts[max_bits as usize][max_bits as usize] != n {
            panic!("leafCounts[maxBits][maxBits] != n");
        }

        let bit_count = &mut self.bit_count[..(max_bits as usize + 1)];
        let counts = &leaf_counts[max_bits as usize];
        for (bits, level) in (1usize..).zip((1..=max_bits).rev()) {
            let lv = level as usize;
            bit_count[bits] = counts[lv] - counts[lv - 1];
        }
        bit_count
    }

    /// Walk the bit-length array and assign actual code values to each
    /// literal. Direct port of Go's `assignEncodingAndSize`.
    fn assign_encoding_and_size(&mut self, bit_count: &[i32], list: &mut [LiteralNode]) {
        let mut code: u16 = 0;
        let mut list = list;
        for (n, &bits) in bit_count.iter().enumerate() {
            code <<= 1;
            if n == 0 || bits == 0 {
                continue;
            }
            // The literals list[len-bits..] are encoded using `n` bits and
            // get the values code, code+1, .... Code values are assigned in
            // literal order (not frequency order), hence the sort.
            let split = list.len() - bits as usize;
            let chunk = &mut list[split..];
            chunk.sort_by_key(|node| node.literal);
            for node in chunk.iter() {
                self.codes[node.literal as usize] = HCode {
                    code: reverse_bits(code, n as u8),
                    len: n as u16,
                };
                code += 1;
            }
            list = &mut list[..split];
        }
    }

    /// Build a Huffman code that minimises the total bit count for the given
    /// frequency table, capped at `max_bits` per code.
    pub(crate) fn generate(&mut self, freq: &[i32], max_bits: i32) {
        if self.freq_cache.is_none() {
            // Largest possible: maxNumLit + 1 sentinel.
            self.freq_cache = Some(vec![
                LiteralNode {
                    literal: 0,
                    freq: 0
                };
                MAX_NUM_LIT + 1
            ]);
        }
        let cache = self.freq_cache.as_mut().unwrap();
        let mut count = 0usize;
        for (i, &f) in freq.iter().enumerate() {
            if f != 0 {
                cache[count] = LiteralNode {
                    literal: i as u16,
                    freq: f,
                };
                count += 1;
            } else {
                self.codes[i].len = 0;
            }
        }

        // Take the populated prefix into a Vec we can mutate freely. We'll
        // discard it on return so the freq_cache buffer remains reusable.
        let mut list: Vec<LiteralNode> = cache[..count].to_vec();
        if count <= 2 {
            // With two or fewer literals everything has bit length 1.
            for (i, node) in list.iter().enumerate() {
                self.codes[node.literal as usize].set(i as u16, 1);
            }
            return;
        }

        // Sort by frequency, breaking ties by literal value (matches Go's
        // `byFreq.Less`).
        list.sort_by(|a, b| {
            if a.freq == b.freq {
                a.literal.cmp(&b.literal)
            } else {
                a.freq.cmp(&b.freq)
            }
        });

        // Compute counts and assign codes.
        let bit_count_len;
        {
            let bit_count = self.bit_counts(&mut list, max_bits);
            bit_count_len = bit_count.len();
        }
        // Drop the sentinel max-node before assignment.
        list.pop();
        // Reborrow bit_count after the sentinel pop (it lives on self).
        let bit_count_owned: Vec<i32> = self.bit_count[..bit_count_len].to_vec();
        self.assign_encoding_and_size(&bit_count_owned, &mut list);
    }
}

/// Build the fixed literal Huffman table from RFC 1951 §3.2.6.
pub(crate) fn generate_fixed_literal_encoding() -> HuffmanEncoder {
    let mut h = HuffmanEncoder::new(MAX_NUM_LIT);
    for ch in 0u16..MAX_NUM_LIT as u16 {
        let (bits, size): (u16, u16) = if ch < 144 {
            (ch + 48, 8)
        } else if ch < 256 {
            (ch + 400 - 144, 9)
        } else if ch < 280 {
            (ch - 256, 7)
        } else {
            (ch + 192 - 280, 8)
        };
        h.codes[ch as usize] = HCode {
            code: reverse_bits(bits, size as u8),
            len: size,
        };
    }
    h
}

pub(crate) fn generate_fixed_offset_encoding() -> HuffmanEncoder {
    let mut h = HuffmanEncoder::new(30);
    for ch in 0..30u16 {
        h.codes[ch as usize] = HCode {
            code: reverse_bits(ch, 5),
            len: 5,
        };
    }
    h
}

#[inline]
pub(crate) fn reverse_bits(number: u16, bit_length: u8) -> u16 {
    (number << (16 - bit_length as u16)).reverse_bits()
}
