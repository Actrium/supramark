//! Block diagram parser.
//!
//! Hand-written recursive-descent port of
//! `packages/mermaid/src/diagrams/block/parser/block.jison`.
//! Accepts both `block-beta` and `block` headers.
//!
//! The input stream is tokenised lazily; tokens are produced on demand
//! by [`Lexer::next`]. Grammar productions match upstream jison's
//! `statement` / `blockStatement` / `nodeStatement` rules.

use crate::error::{MermaidError, Result};
use crate::model::block::{ArrowDir, BlockDiagram, BlockEdge, BlockNode, BlockShape, ClassDef};

/// Convenience builder for a parse error with placeholder line/col.
fn perr(msg: impl Into<String>) -> MermaidError {
    MermaidError::Parse {
        line: 0,
        col: 0,
        message: msg.into(),
    }
}

pub fn parse(source: &str) -> Result<BlockDiagram> {
    let mut p = Parser::new(source);
    p.parse_start()
}

/// Same as `parse` but allows specifying the initial `id_cnt` and `rng_state`.
/// Used to match the cross-fixture cnt accumulation of the batch reference generator.
pub fn parse_with_state(source: &str, id_cnt_start: u64, rng_state: u32) -> Result<BlockDiagram> {
    let mut p = Parser::new(source);
    p.id_cnt = id_cnt_start;
    p.rng_state = rng_state;
    p.parse_start()
}

// ─── Lexer ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// `block-beta` or `block` at the head of the source.
    BlockDiagramKey,
    /// `block:` — opens a named composite.
    IdBlock,
    /// `end` — closes a composite.
    End,
    /// `columns N` or `columns auto` — yields the integer (or -1).
    Columns(i64),
    /// `space`, `space:N` — width defaults to 1.
    SpaceBlock(i64),
    /// `:N` size suffix on a node (width-in-columns).
    Size(i64),
    /// Bare identifier.
    NodeId(String),
    /// Quoted `"..."` string (label text). Contains sanitized inner text.
    Str(String),
    /// One of the shape-opener sequences (`[`, `(`, `((`, `[/`, `{`, `{{`, `>`, `([`, `[[`, `[(`, `(((`).
    NodeDStart(String),
    /// Matching shape-closer (`]`, `)`, etc.).
    NodeDEnd(String),
    /// `<[` — block arrow opener.
    BlockArrowStart,
    /// `]>` ... `(` ... `)` — collapsed to this single token post-close.
    BlockArrowEnd,
    /// Direction word inside `<[...]>(...)`.
    Dir(ArrowDir),
    /// `A --> B`, `A --x B` etc. Full edge body including optional label spacing.
    Link {
        typestr: String,
        label: Option<String>,
    },
    /// `classDef ID css` — emits ID + css segments.
    ClassDef(String, String),
    /// `class A,B name` — ids + style class.
    ApplyClass(String, String),
    /// `style A fill:...` — id + css body.
    Style(String, String),
    /// End of stream.
    Eof,
    /// Line break.
    Newline,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let b = self.peek();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }
    fn rest(&self) -> &str {
        std::str::from_utf8(&self.src[self.pos..]).unwrap_or("")
    }

    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    /// Skip blanks (space/tab) and `%% ... \n` comments, but NOT newlines.
    fn skip_blanks(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => {
                    self.pos += 1;
                }
                Some(b'%') if self.rest().starts_with("%%") => {
                    // Consume until newline
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// Read until a specific byte (exclusive), returning owned content.
    fn read_until_byte(&mut self, stop: u8) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == stop {
                break;
            }
            self.pos += 1;
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    /// Read a quoted `"..."` string. Current pos is on the opening quote.
    fn read_quoted(&mut self) -> String {
        debug_assert_eq!(self.peek(), Some(b'"'));
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'"' {
                break;
            }
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string();
        if self.peek() == Some(b'"') {
            self.pos += 1;
        }
        s
    }

    /// Consume `keyword` if present as a whole word.
    fn consume_keyword(&mut self, kw: &str) -> bool {
        let len = kw.len();
        let bytes = self.src;
        if bytes.len() - self.pos < len {
            return false;
        }
        if &bytes[self.pos..self.pos + len] != kw.as_bytes() {
            return false;
        }
        // Require non-identifier-char boundary after.
        let after = bytes.get(self.pos + len).copied();
        match after {
            None => {}
            Some(c) if is_id_boundary(c) => {}
            _ => return false,
        }
        self.pos += len;
        true
    }

    fn next(&mut self) -> Result<Tok> {
        self.skip_blanks();
        match self.peek() {
            None => Ok(Tok::Eof),
            Some(b'\n') => {
                self.pos += 1;
                Ok(Tok::Newline)
            }
            Some(b':') => {
                // `:N` size suffix.
                self.pos += 1;
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let n: i64 = std::str::from_utf8(&self.src[start..self.pos])
                    .unwrap_or("1")
                    .parse()
                    .unwrap_or(1);
                Ok(Tok::Size(n))
            }
            Some(b'"') => Ok(Tok::Str(self.read_quoted())),
            _ => {
                // Keywords and identifiers.
                if self.consume_keyword("block-beta") {
                    return Ok(Tok::BlockDiagramKey);
                }
                if self.starts_with("block:") {
                    self.pos += 6;
                    return Ok(Tok::IdBlock);
                }
                if self.consume_keyword("block") {
                    return Ok(Tok::BlockDiagramKey);
                }
                if self.consume_keyword("end") {
                    return Ok(Tok::End);
                }
                // `columns N` / `columns auto`
                if self.starts_with("columns")
                    && matches!(self.src.get(self.pos + 7).copied(), Some(b' ' | b'\t'))
                {
                    self.pos += 7;
                    // Consume whitespace then value.
                    while matches!(self.peek(), Some(b' ' | b'\t')) {
                        self.pos += 1;
                    }
                    if self.starts_with("auto") {
                        self.pos += 4;
                        return Ok(Tok::Columns(-1));
                    }
                    let start = self.pos;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                        self.pos += 1;
                    }
                    let n: i64 = std::str::from_utf8(&self.src[start..self.pos])
                        .unwrap_or("1")
                        .parse()
                        .unwrap_or(1);
                    return Ok(Tok::Columns(n));
                }
                // `space:N` or `space`
                if self.starts_with("space:") {
                    self.pos += 6;
                    let start = self.pos;
                    while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                        self.pos += 1;
                    }
                    let n: i64 = std::str::from_utf8(&self.src[start..self.pos])
                        .unwrap_or("1")
                        .parse()
                        .unwrap_or(1);
                    return Ok(Tok::SpaceBlock(n));
                }
                if self.consume_keyword("space") {
                    return Ok(Tok::SpaceBlock(1));
                }
                // classDef / class / style
                if self.consume_keyword("classDef") {
                    self.skip_blanks();
                    let id = self.read_ident_word();
                    self.skip_blanks();
                    let css = self.read_until_byte(b'\n');
                    return Ok(Tok::ClassDef(id, css.trim().to_string()));
                }
                if self.consume_keyword("class") {
                    self.skip_blanks();
                    let ids = self.read_class_ids();
                    self.skip_blanks();
                    let style = self.read_until_byte(b'\n');
                    return Ok(Tok::ApplyClass(ids, style.trim().to_string()));
                }
                if self.consume_keyword("style") {
                    self.skip_blanks();
                    let ids = self.read_class_ids();
                    self.skip_blanks();
                    let style = self.read_until_byte(b'\n');
                    return Ok(Tok::Style(ids, style.trim().to_string()));
                }
                // Shape openers / block arrow / edges / identifiers.
                // Order matters — longer sequences first.
                if self.starts_with("<[") {
                    self.pos += 2;
                    return Ok(Tok::BlockArrowStart);
                }
                // Edge links. Match `<?[xo<]?--+[->xo]? ...` and `==`, etc.
                if let Some(link) = self.try_read_link()? {
                    return Ok(link);
                }
                // Node delimiter openers.
                for (tag, _open) in SHAPE_OPENERS {
                    if self.starts_with(tag) {
                        self.pos += tag.len();
                        return Ok(Tok::NodeDStart(tag.to_string()));
                    }
                }
                // Identifier — run of non-special chars.
                let id = self.read_ident_word();
                if id.is_empty() {
                    let c = self.peek().unwrap_or(b'?');
                    return Err(perr(format!(
                        "block parser: unexpected byte {:?} at pos {}",
                        c as char, self.pos
                    )));
                }
                Ok(Tok::NodeId(id))
            }
        }
    }

    fn read_ident_word(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if is_id_boundary(c) {
                break;
            }
            if matches!(
                c,
                b'(' | b'[' | b'{' | b'}' | b')' | b'<' | b'>' | b':' | b'"'
            ) {
                break;
            }
            self.pos += 1;
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    fn read_class_ids(&mut self) -> String {
        // \w+ (, \s* \w+)*
        let start = self.pos;
        loop {
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
                self.pos += 1;
            }
            if self.peek() == Some(b',') {
                self.pos += 1;
                while matches!(self.peek(), Some(b' ' | b'\t')) {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// Try parsing an edge token. Upstream jison regex:
    /// `\s*[xo<]?\-\-+[-xo>]\s*` or `==`-variant or dotted or `~~~~`.
    /// Also `START_LINK` with a `"..."` label inside.
    fn try_read_link(&mut self) -> Result<Option<Tok>> {
        let save = self.pos;
        let bytes = self.src;
        let i = self.pos;
        // Optional lead x/o/< followed by `-- ... ->|x|o` or `== ... =>|x|o`
        let lead = bytes.get(i).copied();
        let mut j = i;
        if matches!(lead, Some(b'x' | b'o' | b'<')) {
            j += 1;
        }
        // Now expect `--` or `==` or `-.`.
        let dashes = bytes.get(j).copied();
        if !matches!(dashes, Some(b'-' | b'=')) {
            return Ok(None);
        }
        let dash_c = dashes.unwrap();
        let mut k = j;
        while bytes.get(k).copied() == Some(dash_c) {
            k += 1;
        }
        if k - j < 2 {
            return Ok(None);
        }
        // Undirected link: 3+ dashes followed by non-link char (whitespace
        // or identifier). Consume the run as `---`-style link.
        let next = bytes.get(k).copied();
        if dash_c == b'-' && k - j >= 3 && !matches!(next, Some(b'x' | b'o' | b'>' | b'-' | b'=')) {
            self.pos = k;
            let typestr = std::str::from_utf8(&bytes[i..k])
                .unwrap_or("")
                .trim()
                .to_string();
            return Ok(Some(Tok::Link {
                typestr,
                label: None,
            }));
        }
        // Case 1: immediate close `->x|o|>`
        if matches!(next, Some(b'x' | b'o' | b'>' | b'-' | b'=')) && k - j >= 2 {
            // try closing chars: allow e.g. `--x`, `-->`, `---`, `==>`
            // But not `--"label"` scenarios. If next is dash/equal continue consuming
            // then check for final char.
            let mut m = k;
            while bytes.get(m).copied() == Some(dash_c) {
                m += 1;
            }
            let final_c = bytes.get(m).copied();
            if matches!(final_c, Some(b'x' | b'o' | b'>')) {
                // consume final, return full link
                self.pos = m + 1;
                let typestr = std::str::from_utf8(&bytes[i..self.pos])
                    .unwrap_or("")
                    .trim()
                    .to_string();
                return Ok(Some(Tok::Link {
                    typestr,
                    label: None,
                }));
            } else if dash_c == b'-' && bytes.get(m - 1).copied() == Some(b'-') {
                // Plain `---` (n dashes, no arrowhead) — undirected.
                self.pos = m;
                let typestr = std::str::from_utf8(&bytes[i..self.pos])
                    .unwrap_or("")
                    .trim()
                    .to_string();
                return Ok(Some(Tok::Link {
                    typestr,
                    label: None,
                }));
            }
        }
        // Case 2: `--` then whitespace + `"label"` + space + close link.
        // jison's START_LINK: `\s*[xo<]?\-\-\s*` or `==` similar.
        // After grabbing k at end of dashes, if 2 dashes exactly and next is whitespace or quote.
        if k - j == 2 && matches!(next, Some(b' ' | b'\t' | b'"')) {
            // Skip whitespace, read `"..."`, skip whitespace, then expect close link `-->`.
            let mut p = k;
            while matches!(bytes.get(p).copied(), Some(b' ' | b'\t')) {
                p += 1;
            }
            if bytes.get(p).copied() != Some(b'"') {
                self.pos = save;
                return Ok(None);
            }
            // read string
            p += 1;
            let lstart = p;
            while matches!(bytes.get(p).copied(), Some(c) if c != b'"') {
                p += 1;
            }
            let label = std::str::from_utf8(&bytes[lstart..p])
                .unwrap_or("")
                .to_string();
            if bytes.get(p).copied() != Some(b'"') {
                self.pos = save;
                return Ok(None);
            }
            p += 1;
            while matches!(bytes.get(p).copied(), Some(b' ' | b'\t')) {
                p += 1;
            }
            // Now expect closing link.
            let lead2 = bytes.get(p).copied();
            let mut q = p;
            if matches!(lead2, Some(b'x' | b'o' | b'<')) {
                q += 1;
            }
            while bytes.get(q).copied() == Some(dash_c) {
                q += 1;
            }
            if q - p < 2 {
                self.pos = save;
                return Ok(None);
            }
            let final_c = bytes.get(q).copied();
            if !matches!(final_c, Some(b'x' | b'o' | b'>')) {
                self.pos = save;
                return Ok(None);
            }
            self.pos = q + 1;
            // typestr combining start+end — upstream LINK token from LLABEL match.
            let typestr = std::str::from_utf8(&bytes[p..self.pos])
                .unwrap_or("")
                .trim()
                .to_string();
            return Ok(Some(Tok::Link {
                typestr,
                label: Some(label),
            }));
        }
        let _ = save;
        Ok(None)
    }
}

/// Non-identifier boundary: whitespace, EOF, common separators.
fn is_id_boundary(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'-' | b'=' | 0)
}

/// `(opener, closer)` pairs mirroring upstream jison NODE shape rules.
/// Order is significant — longest openers first so `(((` wins over `((`.
const SHAPE_OPENERS: &[(&str, &str)] = &[
    ("(((", ")))"),
    ("((", "))"),
    ("([", "])"),
    ("[[", "]]"),
    ("[(", ")]"),
    ("[/", "/]"),
    ("[\\", "\\]"),
    ("{{", "}}"),
    ("[", "]"),
    ("(", ")"),
    ("{", "}"),
    (">", "]"),
];

/// Close sequences we accept for a given opener. Upstream permits e.g.
/// `[\...\]` and `[\.../]` (trapezoid) alternates.
fn close_sequences(open: &str) -> &'static [&'static str] {
    match open {
        "(((" => &[")))"],
        "((" => &["))"],
        "([" => &["])"],
        "[[" => &["]]"],
        "[(" => &[")]"],
        "[/" => &["/]", "\\]"], // `[/ ... /]` lean_right ; `[/ ... \]` trapezoid
        "[\\" => &["\\]", "/]"], // `[\...\]` lean_left ; `[\.../]` inv_trapezoid
        "{{" => &["}}"],
        // Plain `[` may close with `\]` (lean_left / trapezoid_alt) or `/]`
        // (lean_right_alt) in addition to the standard `]`.
        "[" => &["]", "\\]", "/]"],
        "(" => &[")"],
        "{" => &["}"],
        ">" => &["]"],
        _ => &[],
    }
}

/// Map the `typeStr` (opener+closer concatenated) to a [`BlockShape`].
/// Mirrors `typeStr2Type` in upstream blockDB.
fn typestr_to_shape(ts: &str) -> BlockShape {
    match ts {
        "[]" => BlockShape::Square,
        "()" => BlockShape::Round,
        "(())" => BlockShape::Circle,
        "(((())))" | "((()))" => BlockShape::DoubleCircle,
        "([])" => BlockShape::Stadium,
        "[[]]" => BlockShape::Subroutine,
        "[()]" => BlockShape::Cylinder,
        "{}" => BlockShape::Diamond,
        "{{}}" => BlockShape::Hexagon,
        "[//]" => BlockShape::LeanRight,
        "[\\\\]" => BlockShape::LeanLeft,
        "[/\\]" => BlockShape::Trapezoid,
        "[\\/]" => BlockShape::InvTrapezoid,
        ">]" => BlockShape::RectLeftInvArrow,
        "<[]>" => BlockShape::BlockArrow,
        _ => BlockShape::Na,
    }
}

// ─── Parser ────────────────────────────────────────────────────────────

struct Parser<'a> {
    lexer: Lexer<'a>,
    /// Last-peeked token (one-lookahead).
    peeked: Option<Tok>,
    /// Auto-incrementing id for anonymous blocks (space, edges).
    id_cnt: u64,
    edges: Vec<BlockEdge>,
    class_defs: Vec<ClassDef>,
    apply_class: Vec<(String, String)>,
    apply_style: Vec<(String, String)>,
    /// Global id table for upstream-compatible dedup across nesting
    /// levels (blockDatabase in blockDB.ts). A duplicate id reached via
    /// a later occurrence is NOT added to the parent's children array.
    global_seen: std::collections::HashSet<String>,
    /// `0x12345678` mulberry32 state — seeded to match the jsdom batch
    /// reference generator (tests/support/generate_ref.mjs).
    rng_state: u32,
    /// Mirrors `edgeCount2` in blockDB.ts: counts how many times an
    /// edge `{src}-{dst}` ID has been seen.  Prepended as `{count}-{id}`.
    edge_count: std::collections::HashMap<String, u32>,
}

/// mulberry32 PRNG — matches upstream (jsdom-seeded) byte for byte.
fn mulberry32_next(state: &mut u32) -> f64 {
    // state = (state + 0x6d2b79f5) | 0; (JS coerces to i32 via |0)
    *state = state.wrapping_add(0x6d2b79f5);
    let mut t: u32 = *state;
    // t = Math.imul(t ^ (t >>> 15), 1 | t);
    let a = t ^ (t >> 15);
    let b = 1u32 | t;
    t = a.wrapping_mul(b);
    // t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    let a2 = t ^ (t >> 7);
    let b2 = 61u32 | t;
    let m = a2.wrapping_mul(b2);
    t = (t.wrapping_add(m)) ^ t;
    // return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    let v = (t ^ (t >> 14)) as f64;
    v / 4294967296.0
}

/// JS `Math.random().toString(36).substr(2, 12)`.
///
/// V8's `Number.toString(radix)` for non-decimal radix uses exact
/// integer (Bignum) arithmetic to emit digits until the ULP boundaries
/// of `x` fall into different base-36 buckets.  At that point the last
/// digit is the one containing `x`'s exact position, with a round-up
/// when the fractional remainder of `x` exceeds half of the current
/// bucket step.
///
/// Algorithm:
///   x = mantissa / 2^shift   (IEEE-754 normal double, shift = 52 - exp)
///   Track lo = 2*mantissa - 1, hi = 2*mantissa + 1, exact = 2*mantissa
///   in units of 1 / (2^(shift+1)).  Each step: multiply all three by 36,
///   extract digit = exact / scale2 (scale2 = 2^(shift+1)).
///   Terminate when lo/scale2 != hi/scale2 (ULP straddles a bucket
///   boundary).  Round up if exact_rem >= scale2/2.
fn js_random_to_base36_prefix(x: f64) -> String {
    if x <= 0.0 {
        return String::new();
    }
    let bits = x.to_bits();
    let raw_exp = ((bits >> 52) & 0x7ff) as i32;
    let exp = raw_exp - 1023;
    let mantissa: u64 = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let shift_i: i32 = 52 - exp;
    // For mulberry32 outputs in (0,1): exp in [-4, -1], shift in [53, 56].
    // scale2 = 2^(shift+1) <= 2^57, fits in u128.
    if shift_i <= 0 || shift_i >= 63 {
        return String::new();
    }
    let shift = shift_i as u32;

    // scale2 = 2^(shift+1).  After modulo, values < scale2, so *36 < 36*2^63 < 2^128.
    let scale2: u128 = 1u128 << (shift + 1);
    let mut lo: u128 = 2 * (mantissa as u128) - 1;
    let mut hi: u128 = 2 * (mantissa as u128) + 1;
    let mut exact: u128 = 2 * (mantissa as u128);

    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = String::with_capacity(12);
    for _ in 0..12 {
        lo *= 36;
        hi *= 36;
        exact *= 36;
        let lo_d = lo / scale2;
        let hi_d = hi / scale2;
        let ex_d = exact / scale2;
        lo %= scale2;
        hi %= scale2;
        exact %= scale2;

        // Determine digit: exact position, with round-up when remainder
        // exceeds the halfway point of the bucket.
        let mut digit = ex_d as usize;
        if lo_d != hi_d {
            // ULP straddles a bucket boundary — this is the last digit.
            if exact >= scale2 / 2 {
                digit = digit.saturating_add(1);
            }
            if digit >= 36 {
                out.push('z');
            } else {
                out.push(DIGITS[digit] as char);
            }
            break;
        }
        if digit >= 36 {
            out.push('z');
        } else {
            out.push(DIGITS[digit] as char);
        }
    }
    out
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        // Strip common YAML frontmatter / mermaid directives in line form.
        // Upstream's DiagramPreprocessor handles this; for our tests the
        // source arrives raw. We do a minimal comment sweep instead.
        Self {
            lexer: Lexer::new(src),
            peeked: None,
            id_cnt: 0,
            edges: Vec::new(),
            class_defs: Vec::new(),
            apply_class: Vec::new(),
            apply_style: Vec::new(),
            global_seen: std::collections::HashSet::new(),
            rng_state: 0x12345678,
            edge_count: std::collections::HashMap::new(),
        }
    }

    fn next_tok(&mut self) -> Result<Tok> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        self.lexer.next()
    }

    fn peek_tok(&mut self) -> Result<&Tok> {
        if self.peeked.is_none() {
            self.peeked = Some(self.lexer.next()?);
        }
        Ok(self.peeked.as_ref().unwrap())
    }

    fn skip_newlines(&mut self) -> Result<()> {
        loop {
            match self.peek_tok()? {
                Tok::Newline => {
                    self.next_tok()?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn new_anon_id(&mut self, prefix: &str) -> String {
        self.id_cnt += 1;
        format!("{}-{}", prefix, self.id_cnt)
    }

    /// Generate an upstream-style composite id:
    /// `id-{mulberry32-toBase36-12char}-{cnt}`. Matches
    /// `blockDB.ts::generateId` fed by the jsdom batch seed
    /// (`__rngState = 0x12345678`, see tests/support/generate_ref.mjs).
    fn new_composite_id(&mut self) -> String {
        self.id_cnt += 1;
        let rng = mulberry32_next(&mut self.rng_state);
        format!("id-{}-{}", js_random_to_base36_prefix(rng), self.id_cnt)
    }

    fn parse_start(&mut self) -> Result<BlockDiagram> {
        self.skip_newlines()?;
        let head = self.next_tok()?;
        if head != Tok::BlockDiagramKey {
            return Err(perr(format!(
                "block parser: expected 'block' / 'block-beta' header; got {:?}",
                head
            )));
        }
        let children = self.parse_document()?;
        let mut root = BlockNode {
            id: "root".into(),
            shape: BlockShape::Composite,
            columns: None,
            children,
            width_in_columns: 1,
            ..Default::default()
        };
        // Populate + extract column-settings in children of each composite.
        self.populate(&mut root);
        // Apply classDef bindings.
        let apply_class = std::mem::take(&mut self.apply_class);
        for (id_list, css_class) in apply_class {
            for id in id_list
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                if let Some(n) = find_node_mut(&mut root, id) {
                    n.classes.push(css_class.clone());
                }
            }
        }
        let apply_style = std::mem::take(&mut self.apply_style);
        for (id_list, css) in apply_style {
            for id in id_list
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                if let Some(n) = find_node_mut(&mut root, id) {
                    for s in css.split(',') {
                        n.styles.push(s.trim().to_string());
                    }
                }
            }
        }
        Ok(BlockDiagram {
            meta: Default::default(),
            root,
            edges: std::mem::take(&mut self.edges),
            class_defs: std::mem::take(&mut self.class_defs),
        })
    }

    /// Upstream populateBlockDatabase: extract column-setting siblings,
    /// expand `space:N` into N anonymous siblings, dedupe same-id refs.
    fn populate(&mut self, parent: &mut BlockNode) {
        // 1. Pull out any column-setting child and apply to parent.columns.
        let mut columns_found = None;
        let mut cleaned: Vec<BlockNode> = Vec::with_capacity(parent.children.len());
        for child in std::mem::take(&mut parent.children) {
            if child.shape == BlockShape::Composite && child.id == "__column_setting__" {
                columns_found = Some(child.columns.unwrap_or(-1));
                continue;
            }
            cleaned.push(child);
        }
        if let Some(c) = columns_found {
            parent.columns = Some(c);
        }
        // 2. Default labels: leaf shapes default to id, composite to "".
        //    Matches upstream blockDB.populateBlockDatabase.
        for child in cleaned.iter_mut() {
            if child.label.is_none() {
                child.label = Some(if child.shape == BlockShape::Composite {
                    String::new()
                } else {
                    child.id.clone()
                });
            }
        }
        // 3. Expand `space:N` + recurse into composites.
        //    Dedup against the global id table (`self.seen_ids`) —
        //    upstream stores all blocks in one flat Map so a re-used id
        //    from an edge lookup collides with a previously-defined node.
        let mut expanded: Vec<BlockNode> = Vec::with_capacity(cleaned.len());
        for mut child in cleaned {
            if child.shape == BlockShape::Space {
                let w = child.space_width.max(1);
                for j in 0..w {
                    let mut c = child.clone();
                    c.id = format!("{}-{}", child.id, j);
                    c.width_in_columns = 1;
                    // Register space ids globally so later refs dedupe.
                    self.global_seen.insert(c.id.clone());
                    expanded.push(c);
                }
                continue;
            }
            if child.shape == BlockShape::Composite {
                // Register composite first so its recursive populate
                // sees its own id. Recurse before dedup check — this
                // matches upstream's depth-first registration order.
                if !self.global_seen.insert(child.id.clone()) {
                    // Composite id already seen — still recurse to pull
                    // children in but skip appending duplicate.
                    self.populate(&mut child);
                    continue;
                }
                self.populate(&mut child);
                expanded.push(child);
                continue;
            }
            // Leaf: dedupe against global set.
            if !self.global_seen.insert(child.id.clone()) {
                continue;
            }
            expanded.push(child);
        }
        parent.children = expanded;
    }

    fn parse_document(&mut self) -> Result<Vec<BlockNode>> {
        let mut out = Vec::new();
        self.skip_newlines()?;
        loop {
            match self.peek_tok()? {
                Tok::Eof | Tok::End => break,
                Tok::Newline => {
                    self.next_tok()?;
                    continue;
                }
                _ => {
                    let stmts = self.parse_statement()?;
                    out.extend(stmts);
                    // Optional newline/EOF separator.
                    if matches!(self.peek_tok()?, Tok::Newline) {
                        self.next_tok()?;
                    }
                }
            }
        }
        Ok(out)
    }

    fn parse_statement(&mut self) -> Result<Vec<BlockNode>> {
        let t = self.peek_tok()?.clone();
        match t {
            Tok::Columns(n) => {
                self.next_tok()?;
                // Sentinel node — extracted by populate().
                let mut n_node = BlockNode::default();
                n_node.id = "__column_setting__".into();
                n_node.shape = BlockShape::Composite;
                n_node.columns = Some(n);
                Ok(vec![n_node])
            }
            Tok::SpaceBlock(w) => {
                self.next_tok()?;
                // Upstream also calls generateId() for space tokens.
                let id = self.new_composite_id();
                Ok(vec![BlockNode {
                    id,
                    shape: BlockShape::Space,
                    space_width: w,
                    width_in_columns: 1,
                    ..Default::default()
                }])
            }
            Tok::IdBlock => {
                self.next_tok()?;
                // After `block:` expect a nodeStatement (optionally
                // followed by `:N` SIZE) then the composite body.
                let mut node = self.parse_node()?;
                if let Tok::Size(n) = self.peek_tok()? {
                    let n = *n;
                    self.next_tok()?;
                    node.width_in_columns = n;
                }
                let children = self.parse_document()?;
                match self.next_tok()? {
                    Tok::End => {}
                    other => {
                        return Err(perr(format!(
                            "block parser: expected 'end' after block body; got {:?}",
                            other
                        )))
                    }
                }
                let mut composite = node;
                composite.shape = BlockShape::Composite;
                composite.children = children;
                if composite.label.is_none() {
                    composite.label = Some(String::new());
                }
                Ok(vec![composite])
            }
            Tok::BlockDiagramKey => {
                // `block` nested (no `:`). Upstream's blockStatement variant.
                self.next_tok()?;
                let children = self.parse_document()?;
                match self.next_tok()? {
                    Tok::End => {}
                    other => {
                        return Err(perr(format!(
                            "block parser: expected 'end' after block body; got {:?}",
                            other
                        )))
                    }
                }
                let id = self.new_composite_id();
                Ok(vec![BlockNode {
                    id,
                    shape: BlockShape::Composite,
                    label: Some(String::new()),
                    width_in_columns: 1,
                    children,
                    ..Default::default()
                }])
            }
            Tok::ClassDef(id, css) => {
                self.next_tok()?;
                self.class_defs.push(ClassDef { id, styles: css });
                Ok(vec![])
            }
            Tok::ApplyClass(ids, style) => {
                self.next_tok()?;
                self.apply_class.push((ids, style));
                Ok(vec![])
            }
            Tok::Style(ids, css) => {
                self.next_tok()?;
                self.apply_style.push((ids, css));
                Ok(vec![])
            }
            _ => self.parse_node_statement(),
        }
    }

    /// Parse `node (link node)*` [SIZE]. Upstream nodeStatement.
    fn parse_node_statement(&mut self) -> Result<Vec<BlockNode>> {
        let first = self.parse_node()?;
        // Look for a link token — if present, this is a chain.
        let mut out = Vec::new();
        let mut current = first;
        loop {
            match self.peek_tok()? {
                Tok::Link { .. } => {
                    let link_tok = self.next_tok()?;
                    if let Tok::Link { typestr, label } = link_tok {
                        let next_node = self.parse_node()?;
                        let edge_data = edge_str_to_edge_data(&typestr);
                        // Upstream blockDB.ts prepends an occurrence count to
                        // disambiguate repeated src-dst pairs: id = count + "-" + base_id.
                        let base_id = format!("{}-{}", current.id, next_node.id);
                        let count = {
                            let c = self.edge_count.entry(base_id.clone()).or_insert(0);
                            *c += 1;
                            *c
                        };
                        let edge_id = format!("{}-{}", count, base_id);
                        self.edges.push(BlockEdge {
                            id: edge_id,
                            start: current.id.clone(),
                            end: next_node.id.clone(),
                            label,
                            arrow_type_end: edge_data,
                            arrow_type_start: "arrow_open".into(),
                        });
                        // Emit leading node (without width_in_columns set by SIZE).
                        out.push(BlockNode {
                            id: current.id.clone(),
                            label: current.label.clone(),
                            shape: current.shape,
                            width_in_columns: 1,
                            arrow_dirs: current.arrow_dirs.clone(),
                            ..Default::default()
                        });
                        current = next_node;
                    }
                }
                _ => break,
            }
        }
        // The terminal node may have a SIZE suffix.
        if let Tok::Size(n) = self.peek_tok()? {
            let n = *n;
            self.next_tok()?;
            current.width_in_columns = n;
        } else if current.width_in_columns == 0 {
            current.width_in_columns = 1;
        }
        out.push(current);
        Ok(out)
    }

    /// Parse a single node: `ID [shape-body]`.
    fn parse_node(&mut self) -> Result<BlockNode> {
        let t = self.next_tok()?;
        let id = match t {
            Tok::NodeId(s) => s,
            other => {
                return Err(perr(format!(
                    "block parser: expected node id; got {:?}",
                    other
                )))
            }
        };
        let mut node = BlockNode {
            id,
            width_in_columns: 1,
            ..Default::default()
        };
        // Optional shape-body.
        match self.peek_tok()? {
            Tok::NodeDStart(open) => {
                let open = open.clone();
                self.next_tok()?;
                // Expect a quoted label.
                let label = match self.next_tok()? {
                    Tok::Str(s) => s,
                    other => {
                        return Err(perr(format!(
                            "block parser: expected quoted label inside shape; got {:?}",
                            other
                        )))
                    }
                };
                // Expect matching close.
                let closers = close_sequences(&open);
                let close = self.read_close(closers)?;
                let ts = format!("{}{}", open, close);
                let shape = typestr_to_shape(&ts);
                node.label = Some(label);
                node.shape = shape;
            }
            Tok::BlockArrowStart => {
                self.next_tok()?;
                let label = match self.next_tok()? {
                    Tok::Str(s) => s,
                    other => {
                        return Err(perr(format!(
                            "block parser: expected label in block arrow; got {:?}",
                            other
                        )))
                    }
                };
                // Expect `]>` immediately — our lexer didn't tokenize that, so scan raw.
                // Consume until we see `]>`, then collect directions until `)`.
                let dirs = self.read_block_arrow_tail()?;
                node.label = Some(label);
                node.shape = BlockShape::BlockArrow;
                node.arrow_dirs = Some(dirs);
            }
            _ => {}
        }
        Ok(node)
    }

    /// Consume a shape closer, returning the matched string.
    fn read_close(&mut self, accepted: &[&'static str]) -> Result<String> {
        for &seq in accepted {
            if self.lexer.starts_with(seq) {
                self.lexer.pos += seq.len();
                return Ok(seq.to_string());
            }
        }
        Err(perr(format!(
            "block parser: expected one of {:?} closers at pos {}",
            accepted, self.lexer.pos
        )))
    }

    /// After `<["label"` already consumed, read `]>` `(` dir (, dir)* `)`.
    fn read_block_arrow_tail(&mut self) -> Result<Vec<ArrowDir>> {
        // Skip spaces.
        while matches!(self.lexer.peek(), Some(b' ' | b'\t')) {
            self.lexer.pos += 1;
        }
        if !self.lexer.starts_with("]>") {
            return Err(perr("block arrow: expected ']>'"));
        }
        self.lexer.pos += 2;
        while matches!(self.lexer.peek(), Some(b' ' | b'\t')) {
            self.lexer.pos += 1;
        }
        if self.lexer.peek() != Some(b'(') {
            return Err(perr("block arrow: expected '('"));
        }
        self.lexer.pos += 1;
        let mut dirs = Vec::new();
        loop {
            while matches!(self.lexer.peek(), Some(b' ' | b'\t' | b',')) {
                self.lexer.pos += 1;
            }
            if self.lexer.peek() == Some(b')') {
                self.lexer.pos += 1;
                break;
            }
            // Direction token — letters only (so `x,` splits at `,`).
            let start = self.lexer.pos;
            while let Some(c) = self.lexer.peek() {
                if c.is_ascii_alphabetic() {
                    self.lexer.pos += 1;
                } else {
                    break;
                }
            }
            let w = std::str::from_utf8(&self.lexer.src[start..self.lexer.pos])
                .unwrap_or("")
                .to_string();
            let d = match w.as_str() {
                "up" => ArrowDir::Up,
                "down" => ArrowDir::Down,
                "left" => ArrowDir::Left,
                "right" => ArrowDir::Right,
                "x" => ArrowDir::X,
                "y" => ArrowDir::Y,
                _ => return Err(perr(format!("block arrow: unknown dir {w:?}"))),
            };
            dirs.push(d);
        }
        Ok(dirs)
    }
}

fn edge_str_to_edge_data(ts: &str) -> String {
    let trimmed = ts
        .trim()
        .trim_matches(|c: char| c == '-' || c == '=' || c == ' ');
    match trimmed {
        "x" => "arrow_cross".into(),
        "o" => "arrow_circle".into(),
        ">" => "arrow_point".into(),
        _ => String::new(),
    }
}

fn find_node_mut<'a>(root: &'a mut BlockNode, id: &str) -> Option<&'a mut BlockNode> {
    if root.id == id {
        return Some(root);
    }
    for child in &mut root.children {
        if let Some(n) = find_node_mut(child, id) {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_row() {
        let d = parse("block\n    A\n    B\n    C\n").unwrap();
        assert_eq!(d.root.children.len(), 3);
        assert_eq!(d.root.children[0].id, "A");
    }

    #[test]
    fn parse_columns() {
        let d = parse("block\n  columns 3\n  A B C D\n").unwrap();
        assert_eq!(d.root.columns, Some(3));
        assert_eq!(d.root.children.len(), 4);
    }

    #[test]
    fn parse_size_suffix() {
        let d = parse("block\n  A:2\n").unwrap();
        assert_eq!(d.root.children[0].width_in_columns, 2);
    }

    #[test]
    fn parse_space_expand() {
        let d = parse("block\n  columns 3\n  space:3\n  A\n").unwrap();
        // space:3 should expand to 3 siblings.
        let space_count = d
            .root
            .children
            .iter()
            .filter(|c| c.shape == BlockShape::Space)
            .count();
        assert_eq!(space_count, 3);
    }

    #[test]
    fn parse_nested_block() {
        let src = "block\n  block\n    A\n  end\n  B\n";
        let d = parse(src).unwrap();
        assert_eq!(d.root.children.len(), 2);
        assert_eq!(d.root.children[0].shape, BlockShape::Composite);
        assert_eq!(d.root.children[0].children.len(), 1);
    }

    #[test]
    fn parse_shapes() {
        let d = parse("block\n  A[\"sq\"]\n  B(\"r\")\n  C((\"c\"))\n  D{\"d\"}\n  E{{\"h\"}}\n")
            .unwrap();
        assert_eq!(d.root.children[0].shape, BlockShape::Square);
        assert_eq!(d.root.children[1].shape, BlockShape::Round);
        assert_eq!(d.root.children[2].shape, BlockShape::Circle);
        assert_eq!(d.root.children[3].shape, BlockShape::Diamond);
        assert_eq!(d.root.children[4].shape, BlockShape::Hexagon);
    }

    #[test]
    fn parse_edge() {
        let d = parse("block\n  A --> B\n").unwrap();
        assert_eq!(d.edges.len(), 1);
        assert_eq!(d.edges[0].start, "A");
        assert_eq!(d.edges[0].end, "B");
        assert_eq!(d.edges[0].arrow_type_end, "arrow_point");
    }
}
