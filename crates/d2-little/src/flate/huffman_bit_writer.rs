//! Writes deflate blocks using literal/length and distance Huffman codes.
//!
//! Faithful port of Go's `compress/flate/huffman_bit_writer.go`. The bit
//! buffering, block-type selection, and dynamic-Huffman header encoding all
//! must match Go's behavior to produce byte-identical output.

use crate::flate::consts::{
    BAD_CODE, BUFFER_FLUSH_SIZE, BUFFER_SIZE, CODEGEN_CODE_COUNT, END_BLOCK_MARKER,
    LENGTH_CODES_START, MAX_NUM_LIT, MAX_STORE_BLOCK_SIZE, OFFSET_CODE_COUNT,
};
use crate::flate::huffman_code::{
    HCode, HuffmanEncoder, generate_fixed_literal_encoding, generate_fixed_offset_encoding,
};
use crate::flate::token::{Token, length_code, offset_code};

// ---------------------------------------------------------------------------
// Length / offset extra-bits and base tables (RFC 1951 section 3.2.5)
// ---------------------------------------------------------------------------

pub(crate) static LENGTH_EXTRA_BITS: [i8; 29] = [
    /* 257 */ 0, 0, 0, /* 260 */ 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, /* 270 */ 2, 2, 2, 3,
    3, 3, 3, 4, 4, 4, /* 280 */ 4, 5, 5, 5, 5, 0,
];

pub(crate) static LENGTH_BASE: [u32; 29] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48, 56, 64, 80, 96, 112, 128,
    160, 192, 224, 255,
];

pub(crate) static OFFSET_EXTRA_BITS: [i8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

pub(crate) static OFFSET_BASE: [u32; 30] = [
    0x000000, 0x000001, 0x000002, 0x000003, 0x000004, 0x000006, 0x000008, 0x00000c, 0x000010,
    0x000018, 0x000020, 0x000030, 0x000040, 0x000060, 0x000080, 0x0000c0, 0x000100, 0x000180,
    0x000200, 0x000300, 0x000400, 0x000600, 0x000800, 0x000c00, 0x001000, 0x001800, 0x002000,
    0x003000, 0x004000, 0x006000,
];

pub(crate) static CODEGEN_ORDER: [u32; CODEGEN_CODE_COUNT] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

// Lazy fixed encodings (Go uses package-level vars).
fn fixed_literal_encoding() -> &'static HuffmanEncoder {
    use std::sync::OnceLock;
    static CELL: OnceLock<HuffmanEncoder> = OnceLock::new();
    CELL.get_or_init(generate_fixed_literal_encoding)
}

fn fixed_offset_encoding() -> &'static HuffmanEncoder {
    use std::sync::OnceLock;
    static CELL: OnceLock<HuffmanEncoder> = OnceLock::new();
    CELL.get_or_init(generate_fixed_offset_encoding)
}

/// Static offset encoder used for huffman-only encoding (writeBlockHuff).
fn huff_offset() -> &'static HuffmanEncoder {
    use std::sync::OnceLock;
    static CELL: OnceLock<HuffmanEncoder> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut offset_freq = vec![0i32; OFFSET_CODE_COUNT];
        offset_freq[0] = 1;
        let mut h = HuffmanEncoder::new(OFFSET_CODE_COUNT);
        h.generate(&offset_freq, 15);
        h
    })
}

/// Selects which encoding `write_block` settled on. We can't compare two
/// `HuffmanEncoder` references for equality (they don't impl Eq) so we
/// thread this enum through instead. Matches Go's `literalEncoding ==
/// fixedLiteralEncoding` check.
enum SelectedEncoding {
    Fixed,
    Dynamic,
}

pub(crate) struct HuffmanBitWriter {
    pub(crate) out: Vec<u8>,

    /// Bit buffer (low `nbits` bits hold pending output).
    bits: u64,
    nbits: u32,
    /// Pending bytes that haven't been flushed to `out` yet.
    bytes: [u8; BUFFER_SIZE],
    nbytes: usize,

    codegen_freq: [i32; CODEGEN_CODE_COUNT],
    literal_freq: Vec<i32>,
    offset_freq: Vec<i32>,
    codegen: Vec<u8>,
    pub(crate) literal_encoding: HuffmanEncoder,
    pub(crate) offset_encoding: HuffmanEncoder,
    pub(crate) codegen_encoding: HuffmanEncoder,
}

impl HuffmanBitWriter {
    pub(crate) fn new() -> Self {
        Self {
            out: Vec::new(),
            bits: 0,
            nbits: 0,
            bytes: [0; BUFFER_SIZE],
            nbytes: 0,
            codegen_freq: [0; CODEGEN_CODE_COUNT],
            literal_freq: vec![0; MAX_NUM_LIT],
            offset_freq: vec![0; OFFSET_CODE_COUNT],
            codegen: vec![0; MAX_NUM_LIT + OFFSET_CODE_COUNT + 1],
            literal_encoding: HuffmanEncoder::new(MAX_NUM_LIT),
            offset_encoding: HuffmanEncoder::new(OFFSET_CODE_COUNT),
            codegen_encoding: HuffmanEncoder::new(CODEGEN_CODE_COUNT),
        }
    }

    /// Drain the bit buffer into the byte buffer (padding to a byte boundary)
    /// and flush the byte buffer to `out`. Mirrors Go's `huffmanBitWriter.flush`.
    pub(crate) fn flush(&mut self) {
        let mut n = self.nbytes;
        while self.nbits != 0 {
            self.bytes[n] = self.bits as u8;
            self.bits >>= 8;
            if self.nbits > 8 {
                self.nbits -= 8;
            } else {
                self.nbits = 0;
            }
            n += 1;
        }
        self.bits = 0;
        self.write(n);
        self.nbytes = 0;
    }

    /// Append `self.bytes[..n]` to `out` and reset the byte buffer cursor.
    /// Inlined from Go's `write(b []byte)` which simply called the underlying
    /// io.Writer; here `out` is the underlying sink.
    fn write(&mut self, n: usize) {
        self.out.extend_from_slice(&self.bytes[..n]);
    }

    /// Push `nb` low bits of `b` into the buffer. Direct port of `writeBits`.
    fn write_bits(&mut self, b: i32, nb: u32) {
        // Cast through u32 first so the cast is non-extending (matches Go's
        // `uint64(int32)` conversion which sign-extends, then masks).
        self.bits |= (b as u32 as u64) << self.nbits;
        self.nbits += nb;
        if self.nbits >= 48 {
            let bits = self.bits;
            self.bits >>= 48;
            self.nbits -= 48;
            let n = self.nbytes;
            self.bytes[n] = bits as u8;
            self.bytes[n + 1] = (bits >> 8) as u8;
            self.bytes[n + 2] = (bits >> 16) as u8;
            self.bytes[n + 3] = (bits >> 24) as u8;
            self.bytes[n + 4] = (bits >> 32) as u8;
            self.bytes[n + 5] = (bits >> 40) as u8;
            let mut n = n + 6;
            if n >= BUFFER_FLUSH_SIZE {
                self.write(n);
                n = 0;
            }
            self.nbytes = n;
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        let mut n = self.nbytes;
        if self.nbits & 7 != 0 {
            panic!("flate: writeBytes with unfinished bits");
        }
        while self.nbits != 0 {
            self.bytes[n] = self.bits as u8;
            self.bits >>= 8;
            self.nbits -= 8;
            n += 1;
        }
        if n != 0 {
            self.write(n);
        }
        self.nbytes = 0;
        self.out.extend_from_slice(bytes);
    }

    fn write_code(&mut self, c: HCode) {
        self.bits |= (c.code as u64) << self.nbits;
        self.nbits += c.len as u32;
        if self.nbits >= 48 {
            let bits = self.bits;
            self.bits >>= 48;
            self.nbits -= 48;
            let n = self.nbytes;
            self.bytes[n] = bits as u8;
            self.bytes[n + 1] = (bits >> 8) as u8;
            self.bytes[n + 2] = (bits >> 16) as u8;
            self.bytes[n + 3] = (bits >> 24) as u8;
            self.bytes[n + 4] = (bits >> 32) as u8;
            self.bytes[n + 5] = (bits >> 40) as u8;
            let mut n = n + 6;
            if n >= BUFFER_FLUSH_SIZE {
                self.write(n);
                n = 0;
            }
            self.nbytes = n;
        }
    }

    /// RFC 1951 §3.2.7 run-length encoding for the concatenated literal/offset
    /// code-length array. Direct port of `generateCodegen`.
    fn generate_codegen(
        &mut self,
        num_literals: usize,
        num_offsets: usize,
        // For the writeBlockHuff path the offset encoder is the static
        // huff_offset, so we accept a borrow rather than `&self.offset_encoding`.
        lit_codes: &[HCode],
        off_codes: &[HCode],
    ) {
        for v in &mut self.codegen_freq {
            *v = 0;
        }
        // Copy concatenated code sizes into the codegen scratch buffer.
        for i in 0..num_literals {
            self.codegen[i] = lit_codes[i].len as u8;
        }
        for i in 0..num_offsets {
            self.codegen[num_literals + i] = off_codes[i].len as u8;
        }
        self.codegen[num_literals + num_offsets] = BAD_CODE;

        let mut size = self.codegen[0];
        let mut count: i32 = 1;
        let mut out_index: usize = 0;
        let mut in_index: usize = 1;
        loop {
            if size == BAD_CODE {
                break;
            }
            let next_size = self.codegen[in_index];
            in_index += 1;
            if next_size == size {
                count += 1;
                continue;
            }
            // Emit "count copies of size".
            if size != 0 {
                self.codegen[out_index] = size;
                out_index += 1;
                self.codegen_freq[size as usize] += 1;
                count -= 1;
                while count >= 3 {
                    let n = if 6 > count { count } else { 6 };
                    self.codegen[out_index] = 16;
                    out_index += 1;
                    self.codegen[out_index] = (n - 3) as u8;
                    out_index += 1;
                    self.codegen_freq[16] += 1;
                    count -= n;
                }
            } else {
                while count >= 11 {
                    let n = if 138 > count { count } else { 138 };
                    self.codegen[out_index] = 18;
                    out_index += 1;
                    self.codegen[out_index] = (n - 11) as u8;
                    out_index += 1;
                    self.codegen_freq[18] += 1;
                    count -= n;
                }
                if count >= 3 {
                    self.codegen[out_index] = 17;
                    out_index += 1;
                    self.codegen[out_index] = (count - 3) as u8;
                    out_index += 1;
                    self.codegen_freq[17] += 1;
                    count = 0;
                }
            }
            count -= 1;
            while count >= 0 {
                self.codegen[out_index] = size;
                out_index += 1;
                self.codegen_freq[size as usize] += 1;
                count -= 1;
            }
            // Set up invariant for next iteration.
            size = next_size;
            count = 1;
        }
        self.codegen[out_index] = BAD_CODE;
    }

    /// Compute the size in bits of a dynamic-Huffman encoding of the current
    /// literal/offset frequency tables (plus given extra bits).
    /// Returns `(size_in_bits, num_codegens)`.
    fn dynamic_size(
        &self,
        lit_enc: &HuffmanEncoder,
        off_enc: &HuffmanEncoder,
        extra_bits: usize,
    ) -> (usize, usize) {
        let mut num_codegens = self.codegen_freq.len();
        while num_codegens > 4 && self.codegen_freq[CODEGEN_ORDER[num_codegens - 1] as usize] == 0 {
            num_codegens -= 1;
        }
        let header = 3
            + 5
            + 5
            + 4
            + (3 * num_codegens)
            + self.codegen_encoding.bit_length(&self.codegen_freq[..])
            + (self.codegen_freq[16] as usize) * 2
            + (self.codegen_freq[17] as usize) * 3
            + (self.codegen_freq[18] as usize) * 7;
        let size = header
            + lit_enc.bit_length(&self.literal_freq)
            + off_enc.bit_length(&self.offset_freq)
            + extra_bits;
        (size, num_codegens)
    }

    fn fixed_size(&self, extra_bits: usize) -> usize {
        3 + fixed_literal_encoding().bit_length(&self.literal_freq)
            + fixed_offset_encoding().bit_length(&self.offset_freq)
            + extra_bits
    }

    /// `(size_in_bits, storable)` — `storable` is false when `in_buf` is too
    /// large to fit in a single stored block.
    fn stored_size(in_buf: Option<&[u8]>) -> (usize, bool) {
        match in_buf {
            None => (0, false),
            Some(b) if b.len() <= MAX_STORE_BLOCK_SIZE => ((b.len() + 5) * 8, true),
            Some(_) => (0, false),
        }
    }

    /// Write a stored block header. The deflate spec requires the next byte
    /// boundary, so we flush first.
    pub(crate) fn write_stored_header(&mut self, length: usize, is_eof: bool) {
        let flag: i32 = if is_eof { 1 } else { 0 };
        self.write_bits(flag, 3);
        self.flush();
        self.write_bits(length as i32, 16);
        self.write_bits((!(length as u16) as u32) as i32, 16);
    }

    fn write_fixed_header(&mut self, is_eof: bool) {
        // 010 (BTYPE=01, fixed Huffman) for non-final blocks; 011 for final.
        let value: i32 = if is_eof { 3 } else { 2 };
        self.write_bits(value, 3);
    }

    /// Emit the dynamic-Huffman block header (RFC 1951 §3.2.7).
    fn write_dynamic_header(
        &mut self,
        num_literals: usize,
        num_offsets: usize,
        num_codegens: usize,
        is_eof: bool,
    ) {
        let first_bits: i32 = if is_eof { 5 } else { 4 };
        self.write_bits(first_bits, 3);
        self.write_bits((num_literals as i32) - 257, 5);
        self.write_bits((num_offsets as i32) - 1, 5);
        self.write_bits((num_codegens as i32) - 4, 4);

        for i in 0..num_codegens {
            let value = self.codegen_encoding.codes[CODEGEN_ORDER[i] as usize].len as i32;
            self.write_bits(value, 3);
        }

        let mut i = 0usize;
        loop {
            let code_word = self.codegen[i] as usize;
            i += 1;
            if code_word == BAD_CODE as usize {
                break;
            }
            let c = self.codegen_encoding.codes[code_word];
            self.write_code(c);
            match code_word {
                16 => {
                    self.write_bits(self.codegen[i] as i32, 2);
                    i += 1;
                }
                17 => {
                    self.write_bits(self.codegen[i] as i32, 3);
                    i += 1;
                }
                18 => {
                    self.write_bits(self.codegen[i] as i32, 7);
                    i += 1;
                }
                _ => {}
            }
        }
    }

    /// Build literal/offset frequencies from a token stream and generate
    /// Huffman encoders for both. Direct port of `indexTokens`. Returns
    /// `(num_literals, num_offsets)`.
    fn index_tokens(&mut self, tokens: &[Token]) -> (usize, usize) {
        for v in &mut self.literal_freq {
            *v = 0;
        }
        for v in &mut self.offset_freq {
            *v = 0;
        }

        for &t in tokens {
            if !t.is_match() {
                self.literal_freq[t.lit() as usize] += 1;
                continue;
            }
            let length = t.length();
            let offset = t.offset();
            self.literal_freq[LENGTH_CODES_START + length_code(length) as usize] += 1;
            self.offset_freq[offset_code(offset) as usize] += 1;
        }

        let mut num_literals = self.literal_freq.len();
        while self.literal_freq[num_literals - 1] == 0 {
            num_literals -= 1;
        }
        let mut num_offsets = self.offset_freq.len();
        while num_offsets > 0 && self.offset_freq[num_offsets - 1] == 0 {
            num_offsets -= 1;
        }
        if num_offsets == 0 {
            // We haven't found a single match. Force at least one offset to
            // ensure the offset Huffman tree can be encoded.
            self.offset_freq[0] = 1;
            num_offsets = 1;
        }
        self.literal_encoding.generate(&self.literal_freq, 15);
        self.offset_encoding.generate(&self.offset_freq, 15);
        (num_literals, num_offsets)
    }

    /// Emit each token using the chosen literal/offset codes.
    fn write_tokens(&mut self, tokens: &[Token], le_codes: &[HCode], oe_codes: &[HCode]) {
        for &t in tokens {
            if !t.is_match() {
                self.write_code(le_codes[t.lit() as usize]);
                continue;
            }
            let length = t.length();
            let lc = length_code(length);
            self.write_code(le_codes[lc as usize + LENGTH_CODES_START]);
            let extra_length_bits = LENGTH_EXTRA_BITS[lc as usize] as u32;
            if extra_length_bits > 0 {
                let extra_length = length as i32 - LENGTH_BASE[lc as usize] as i32;
                self.write_bits(extra_length, extra_length_bits);
            }
            let offset = t.offset();
            let oc = offset_code(offset);
            self.write_code(oe_codes[oc as usize]);
            let extra_offset_bits = OFFSET_EXTRA_BITS[oc as usize] as u32;
            if extra_offset_bits > 0 {
                let extra_offset = offset as i32 - OFFSET_BASE[oc as usize] as i32;
                self.write_bits(extra_offset, extra_offset_bits);
            }
        }
    }

    /// Pick the smallest of stored/fixed/dynamic encodings and emit the block.
    /// Direct port of `writeBlock`. The caller passes the original input bytes
    /// (when available) so we can compare against a stored block.
    pub(crate) fn write_block(&mut self, tokens: &mut Vec<Token>, eof: bool, input: Option<&[u8]>) {
        tokens.push(Token(END_BLOCK_MARKER as u32));
        let (num_literals, num_offsets) = self.index_tokens(tokens);

        let mut extra_bits: usize = 0;
        let (stored_size_bits, storable) = Self::stored_size(input);
        if storable {
            // Compute extra bits used by length/offset fields. These bits are
            // the same for fixed and dynamic encodings, so we only bother when
            // we actually need to compare against a stored block.
            for length_code_idx in (LENGTH_CODES_START + 8)..num_literals {
                extra_bits += (self.literal_freq[length_code_idx] as usize)
                    * (LENGTH_EXTRA_BITS[length_code_idx - LENGTH_CODES_START] as usize);
            }
            for offset_code_idx in 4..num_offsets {
                extra_bits += (self.offset_freq[offset_code_idx] as usize)
                    * (OFFSET_EXTRA_BITS[offset_code_idx] as usize);
            }
        }

        // Fixed Huffman baseline.
        let mut size = self.fixed_size(extra_bits);
        let mut selected = SelectedEncoding::Fixed;

        // Generate codegen for the dynamic encoding.
        // SAFETY: borrow trick — we need the lit/off encoders both for generate
        // _codegen (read) and codegen_encoding.generate (mutate self). We work
        // around it by cloning the code arrays into the call.
        {
            let lit_codes = self.literal_encoding.codes.clone();
            let off_codes = self.offset_encoding.codes.clone();
            self.generate_codegen(num_literals, num_offsets, &lit_codes, &off_codes);
        }
        self.codegen_encoding
            .generate(&self.codegen_freq.to_vec(), 7);

        let (dynamic_size, num_codegens) =
            self.dynamic_size(&self.literal_encoding, &self.offset_encoding, extra_bits);

        if dynamic_size < size {
            size = dynamic_size;
            selected = SelectedEncoding::Dynamic;
        }

        // Stored bytes?
        if storable && stored_size_bits < size {
            let input = input.unwrap();
            self.write_stored_header(input.len(), eof);
            self.write_bytes(input);
            return;
        }

        // We deferred binding the dynamic code slices because borrow rules
        // wouldn't allow us to hold them across the dynamic_size call. Re-do
        // the selection now using the size we already computed.
        match selected {
            SelectedEncoding::Fixed => {
                self.write_fixed_header(eof);
                let lit_enc = fixed_literal_encoding();
                let off_enc = fixed_offset_encoding();
                let lit = lit_enc.codes.clone();
                let off = off_enc.codes.clone();
                self.write_tokens(tokens, &lit, &off);
            }
            SelectedEncoding::Dynamic => {
                self.write_dynamic_header(num_literals, num_offsets, num_codegens, eof);
                let lit = self.literal_encoding.codes.clone();
                let off = self.offset_encoding.codes.clone();
                self.write_tokens(tokens, &lit, &off);
            }
        }
    }

    /// Direct port of `writeBlockDynamic`. Used by the `BestSpeed` path; not
    /// strictly required for level 6, but kept for completeness.
    #[allow(dead_code)]
    pub(crate) fn write_block_dynamic(
        &mut self,
        tokens: &mut Vec<Token>,
        eof: bool,
        input: Option<&[u8]>,
    ) {
        tokens.push(Token(END_BLOCK_MARKER as u32));
        let (num_literals, num_offsets) = self.index_tokens(tokens);

        {
            let lit_codes = self.literal_encoding.codes.clone();
            let off_codes = self.offset_encoding.codes.clone();
            self.generate_codegen(num_literals, num_offsets, &lit_codes, &off_codes);
        }
        self.codegen_encoding
            .generate(&self.codegen_freq.to_vec(), 7);
        let (size, num_codegens) =
            self.dynamic_size(&self.literal_encoding, &self.offset_encoding, 0);

        if let (ssize, true) = Self::stored_size(input) {
            if ssize < size + (size >> 4) {
                let input = input.unwrap();
                self.write_stored_header(input.len(), eof);
                self.write_bytes(input);
                return;
            }
        }

        self.write_dynamic_header(num_literals, num_offsets, num_codegens, eof);
        let lit = self.literal_encoding.codes.clone();
        let off = self.offset_encoding.codes.clone();
        self.write_tokens(tokens, &lit, &off);
    }

    /// Direct port of `writeBlockHuff`. Used by the `HuffmanOnly` and
    /// `BestSpeed` paths. Not invoked at level 6 in our pipeline.
    #[allow(dead_code)]
    pub(crate) fn write_block_huff(&mut self, eof: bool, input: &[u8]) {
        for v in &mut self.literal_freq {
            *v = 0;
        }
        // histogram
        for &b in input {
            self.literal_freq[b as usize] += 1;
        }
        self.literal_freq[END_BLOCK_MARKER] = 1;

        const NUM_LITERALS: usize = END_BLOCK_MARKER + 1;
        self.offset_freq[0] = 1;
        const NUM_OFFSETS: usize = 1;

        self.literal_encoding.generate(&self.literal_freq, 15);

        let (lit_codes, off_codes) = {
            let l = self.literal_encoding.codes.clone();
            let o = huff_offset().codes.clone();
            (l, o)
        };
        self.generate_codegen(NUM_LITERALS, NUM_OFFSETS, &lit_codes, &off_codes);
        self.codegen_encoding
            .generate(&self.codegen_freq.to_vec(), 7);
        let (size, num_codegens) = self.dynamic_size(&self.literal_encoding, huff_offset(), 0);

        if let (ssize, true) = Self::stored_size(Some(input)) {
            if ssize < size + (size >> 4) {
                self.write_stored_header(input.len(), eof);
                self.write_bytes(input);
                return;
            }
        }

        self.write_dynamic_header(NUM_LITERALS, NUM_OFFSETS, num_codegens, eof);
        let encoding = self.literal_encoding.codes[..257].to_vec();
        for &t in input {
            self.write_code(encoding[t as usize]);
        }
        self.write_code(encoding[END_BLOCK_MARKER]);
    }
}
