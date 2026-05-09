//! LZ77 token type used by the deflate compressor.
//!
//! Faithful port of Go's `compress/flate/token.go`. The token packs three
//! pieces of information into a single u32:
//!
//! * 2 bits — type (literal / EOF marker / match)
//! * 8 bits — extra length (length - MIN_MATCH_LENGTH)
//! * 22 bits — extra offset (offset - MIN_OFFSET_SIZE), or literal value
//!
//! Keeping the bit layout identical to Go is essential for byte-identical
//! deflate output: the bit-writer reads `length()` and `offset()` directly
//! from these tokens.

pub(crate) const LENGTH_SHIFT: u32 = 22;
pub(crate) const OFFSET_MASK: u32 = (1u32 << LENGTH_SHIFT) - 1;
#[allow(dead_code)]
pub(crate) const TYPE_MASK: u32 = 3 << 30;
pub(crate) const LITERAL_TYPE: u32 = 0 << 30;
pub(crate) const MATCH_TYPE: u32 = 1 << 30;

/// length code for length X (MIN_MATCH_LENGTH <= X <= MAX_MATCH_LENGTH)
/// is `LENGTH_CODES[length - MIN_MATCH_LENGTH]`.
pub(crate) static LENGTH_CODES: [u32; 256] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 12, 12, 13, 13, 13, 13, 14, 14, 14,
    14, 15, 15, 15, 15, 16, 16, 16, 16, 16, 16, 16, 16, 17, 17, 17, 17, 17, 17, 17, 17, 18, 18, 18,
    18, 18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20,
    20, 20, 20, 20, 20, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 21, 22, 22, 22,
    22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 22, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23, 23,
    23, 23, 23, 23, 23, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24,
    24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 24, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25,
    25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 25, 26, 26, 26,
    26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26, 26,
    26, 26, 26, 26, 26, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27,
    27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 27, 28,
];

pub(crate) static OFFSET_CODES: [u32; 256] = [
    0, 1, 2, 3, 4, 4, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 9, 9, 9, 9, 9, 9, 9, 9,
    10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11, 11, 11, 11, 11,
    11, 11, 11, 11, 11, 11, 11, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
    14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15,
];

/// A deflate token packs literal/length/offset info into 32 bits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token(pub(crate) u32);

impl Token {
    #[inline]
    pub(crate) const fn literal(literal: u32) -> Token {
        Token(LITERAL_TYPE + literal)
    }

    #[inline]
    pub(crate) const fn match_token(xlength: u32, xoffset: u32) -> Token {
        Token(MATCH_TYPE + (xlength << LENGTH_SHIFT) + xoffset)
    }

    /// Returns the literal of a literal token.
    #[inline]
    pub(crate) const fn lit(self) -> u32 {
        self.0.wrapping_sub(LITERAL_TYPE)
    }

    /// Returns the extra offset of a match token.
    #[inline]
    pub(crate) const fn offset(self) -> u32 {
        self.0 & OFFSET_MASK
    }

    /// Returns the extra length of a match token.
    #[inline]
    pub(crate) const fn length(self) -> u32 {
        (self.0.wrapping_sub(MATCH_TYPE)) >> LENGTH_SHIFT
    }

    /// True if this token is a match (rather than a literal).
    #[inline]
    pub(crate) const fn is_match(self) -> bool {
        self.0 >= MATCH_TYPE
    }
}

#[inline]
pub(crate) fn length_code(len: u32) -> u32 {
    LENGTH_CODES[len as usize]
}

/// Returns the offset code corresponding to a specific offset, mirroring Go's
/// three-tier lookup (small / medium / large offsets).
#[inline]
pub(crate) fn offset_code(off: u32) -> u32 {
    let n = OFFSET_CODES.len() as u32;
    if off < n {
        return OFFSET_CODES[off as usize];
    }
    if off >> 7 < n {
        return OFFSET_CODES[(off >> 7) as usize] + 14;
    }
    OFFSET_CODES[(off >> 14) as usize] + 28
}
