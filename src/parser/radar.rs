//! Hand-rolled parser for the `radar-beta` diagram.
//!
//! Upstream grammar: /ext/mermaid-official-stable-v11.14.0/packages/parser/src/language/radar/radar.langium
//! Upstream DB (post-AST reshape): /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/radar/db.ts
//!
//! Grammar supported (covers every fixture in tests/ext_fixtures/{cypress,demos}/radar):
//!
//! ```text
//! radar-beta[:]                                       # header (optional trailing colon)
//!     title <free text until EOL>                     # optional, zero or more
//!     axis ID[("[label]")] (, ID[("[label]")])*       # zero or more axis groups
//!     curve ID[("[label]")] { entries }               # zero or more curves
//!     <option-name> <value>                           # showLegend / ticks / max / min / graticule
//! ```
//!
//! Entries inside `{ ... }` are either bare numbers (`1, 2, 3`) or axis-qualified
//! (`Agility 2, Speed 2, ...`), optionally separated by newlines.

use crate::error::{MermaidError, Result};
use crate::model::radar::{Graticule, RadarAxis, RadarCurve, RadarDiagram};

/// Parse a mermaid `radar-beta` source and return a `RadarDiagram`.
pub fn parse(source: &str) -> Result<RadarDiagram> {
    let mut p = Parser::new(source);
    p.parse_header()?;

    let mut diagram = RadarDiagram::default();

    loop {
        p.skip_blank_lines();
        if p.at_eof() {
            break;
        }

        let kw = p.peek_keyword();
        match kw.as_deref() {
            Some("title") => {
                p.consume_ident_like("title");
                let title = p.take_line_rest();
                diagram.meta.title = Some(title);
            }
            Some("accTitle") => {
                p.consume_ident_like("accTitle");
                p.skip_inline_ws();
                // Grammar allows `accTitle:` or `accTitle <value>`.
                p.eat_char(':');
                p.skip_inline_ws();
                let val = p.take_line_rest();
                diagram.meta.acc_title = Some(val);
            }
            Some("accDescr") => {
                p.consume_ident_like("accDescr");
                p.skip_inline_ws();
                p.eat_char(':');
                p.skip_inline_ws();
                let val = p.take_line_rest();
                diagram.meta.acc_descr = Some(val);
            }
            Some("axis") => {
                p.consume_ident_like("axis");
                let axes = p.parse_axis_list()?;
                diagram.axes.extend(axes);
            }
            Some("curve") => {
                p.consume_ident_like("curve");
                let curves = p.parse_curve_list(&diagram.axes)?;
                diagram.curves.extend(curves);
            }
            Some("showLegend") => {
                p.consume_ident_like("showLegend");
                p.skip_inline_ws();
                diagram.options.show_legend = p.parse_bool()?;
                p.finish_line()?;
            }
            Some("ticks") => {
                p.consume_ident_like("ticks");
                p.skip_inline_ws();
                let n = p.parse_number()?;
                diagram.options.ticks = n as u32;
                p.finish_line()?;
            }
            Some("max") => {
                p.consume_ident_like("max");
                p.skip_inline_ws();
                diagram.options.max = Some(p.parse_number()?);
                p.finish_line()?;
            }
            Some("min") => {
                p.consume_ident_like("min");
                p.skip_inline_ws();
                diagram.options.min = p.parse_number()?;
                p.finish_line()?;
            }
            Some("graticule") => {
                p.consume_ident_like("graticule");
                p.skip_inline_ws();
                diagram.options.graticule = p.parse_graticule()?;
                p.finish_line()?;
            }
            Some(other) => {
                return Err(p.err(format!("unknown radar keyword: {other}")));
            }
            None => {
                return Err(p.err("expected radar keyword".into()));
            }
        }
    }

    Ok(diagram)
}

// -------------------------------------------------------------------------------------------------
// Low-level parser. Hand-rolled cursor over the source bytes.
// -------------------------------------------------------------------------------------------------

struct Parser<'s> {
    src: &'s str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'s> Parser<'s> {
    fn new(src: &'s str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn err(&self, message: String) -> MermaidError {
        MermaidError::Parse {
            line: self.line,
            col: self.col,
            message,
        }
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek_byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn bump(&mut self) {
        if let Some(b) = self.peek_byte() {
            if b == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }

    fn skip_inline_ws(&mut self) {
        while let Some(b) = self.peek_byte() {
            if b == b' ' || b == b'\t' {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn eat_char(&mut self, c: char) -> bool {
        if self.peek_byte() == Some(c as u8) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Advance past blank lines (whitespace-only) and comment lines (`%%`).
    fn skip_blank_lines(&mut self) {
        loop {
            let save = self.pos;
            self.skip_inline_ws();
            match self.peek_byte() {
                Some(b'\n') => {
                    self.bump();
                }
                Some(b'\r') => {
                    self.bump();
                    if self.peek_byte() == Some(b'\n') {
                        self.bump();
                    }
                }
                Some(b'%') if self.src.as_bytes().get(self.pos + 1) == Some(&b'%') => {
                    // Line comment; consume until newline.
                    while let Some(b) = self.peek_byte() {
                        if b == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                _ => {
                    // Restore so that keyword peek sees leading whitespace context.
                    self.pos = save;
                    // Recompute col cheaply by scanning backwards to line start; but since
                    // skip_inline_ws only consumed spaces/tabs on the current line we can
                    // reset col from save.
                    self.col -= self.pos - save; // inverse only when we did bump on same line
                    // Simpler: recompute line/col by linear scan from file start (rare).
                    self.recompute_line_col();
                    break;
                }
            }
        }
        // Final skip of inline ws so keyword recognition starts clean.
        self.skip_inline_ws();
    }

    fn recompute_line_col(&mut self) {
        let mut line = 1;
        let mut col = 1;
        for b in &self.src.as_bytes()[..self.pos] {
            if *b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        self.line = line;
        self.col = col;
    }

    /// Jump to EOL (consuming trailing `\n` but not further blank lines).
    fn finish_line(&mut self) -> Result<()> {
        self.skip_inline_ws();
        match self.peek_byte() {
            None => Ok(()),
            Some(b'\n') => {
                self.bump();
                Ok(())
            }
            Some(b'\r') => {
                self.bump();
                if self.peek_byte() == Some(b'\n') {
                    self.bump();
                }
                Ok(())
            }
            Some(b',') => Ok(()), // Caller handles comma-separated continuations.
            Some(other) => Err(self.err(format!("unexpected trailing byte 0x{other:02x}"))),
        }
    }

    /// Consume everything up to (but not including) the next `\n`, trimming trailing ws.
    fn take_line_rest(&mut self) -> String {
        self.skip_inline_ws();
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.bump();
        }
        let end = self.pos;
        // Consume the newline itself so subsequent `skip_blank_lines` sees a fresh line.
        if self.peek_byte() == Some(b'\r') {
            self.bump();
        }
        if self.peek_byte() == Some(b'\n') {
            self.bump();
        }
        let raw = &self.src[start..end];
        raw.trim_end().to_string()
    }

    /// Look at the next identifier-like token WITHOUT consuming it.
    /// Returns the token string or None at EOF / non-ident start.
    fn peek_keyword(&self) -> Option<String> {
        let bytes = self.src.as_bytes();
        let mut i = self.pos;
        // skip inline ws
        while let Some(&b) = bytes.get(i) {
            if b == b' ' || b == b'\t' {
                i += 1;
            } else {
                break;
            }
        }
        let start = i;
        while let Some(&b) = bytes.get(i) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                i += 1;
            } else {
                break;
            }
        }
        if start == i {
            None
        } else {
            Some(self.src[start..i].to_string())
        }
    }

    fn consume_ident_like(&mut self, expected: &str) {
        self.skip_inline_ws();
        for _ in 0..expected.len() {
            self.bump();
        }
    }

    /// Parse an identifier `[A-Za-z_][A-Za-z0-9_]*`.
    fn parse_ident(&mut self) -> Result<String> {
        self.skip_inline_ws();
        let start = self.pos;
        match self.peek_byte() {
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => self.bump(),
            _ => return Err(self.err("expected identifier".into())),
        }
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.bump();
            } else {
                break;
            }
        }
        Ok(self.src[start..self.pos].to_string())
    }

    /// Parse an optional bracketed label: `["..."]`.
    fn parse_opt_label(&mut self) -> Result<Option<String>> {
        self.skip_inline_ws();
        if !self.eat_char('[') {
            return Ok(None);
        }
        self.skip_inline_ws();
        if !self.eat_char('"') {
            return Err(self.err("expected '\"' after '[' in label".into()));
        }
        let start = self.pos;
        while let Some(b) = self.peek_byte() {
            if b == b'"' {
                break;
            }
            if b == b'\n' {
                return Err(self.err("unterminated string".into()));
            }
            self.bump();
        }
        let end = self.pos;
        if !self.eat_char('"') {
            return Err(self.err("unterminated string".into()));
        }
        self.skip_inline_ws();
        if !self.eat_char(']') {
            return Err(self.err("expected ']' after label string".into()));
        }
        Ok(Some(self.src[start..end].to_string()))
    }

    /// Parse a signed number (integer or decimal).
    fn parse_number(&mut self) -> Result<f64> {
        self.skip_inline_ws();
        let start = self.pos;
        if self.peek_byte() == Some(b'-') || self.peek_byte() == Some(b'+') {
            self.bump();
        }
        let mut saw_digit = false;
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_digit() {
                saw_digit = true;
                self.bump();
            } else {
                break;
            }
        }
        if self.peek_byte() == Some(b'.') {
            self.bump();
            while let Some(b) = self.peek_byte() {
                if b.is_ascii_digit() {
                    saw_digit = true;
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if !saw_digit {
            return Err(self.err("expected number".into()));
        }
        self.src[start..self.pos]
            .parse::<f64>()
            .map_err(|e| self.err(format!("bad number: {e}")))
    }

    fn parse_bool(&mut self) -> Result<bool> {
        self.skip_inline_ws();
        let id = self.parse_ident()?;
        match id.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(self.err(format!("expected boolean, got {other}"))),
        }
    }

    fn parse_graticule(&mut self) -> Result<Graticule> {
        self.skip_inline_ws();
        let id = self.parse_ident()?;
        match id.as_str() {
            "circle" => Ok(Graticule::Circle),
            "polygon" => Ok(Graticule::Polygon),
            other => Err(self.err(format!("expected 'circle' or 'polygon', got {other}"))),
        }
    }

    /// Parse the initial `radar-beta` header, with an optional trailing colon.
    fn parse_header(&mut self) -> Result<()> {
        // Skip any leading blank lines.
        self.skip_blank_lines();
        let kw = self
            .peek_keyword()
            .ok_or_else(|| self.err("expected 'radar-beta'".into()))?;
        if kw != "radar" {
            return Err(self.err(format!("expected 'radar-beta', got {kw}")));
        }
        // Consume "radar"
        for _ in 0..5 {
            self.bump();
        }
        if !self.eat_char('-') {
            return Err(self.err("expected '-beta' in header".into()));
        }
        let beta = self.parse_ident()?;
        if beta != "beta" {
            return Err(self.err(format!("expected 'beta', got {beta}")));
        }
        self.skip_inline_ws();
        // Optional colon.
        self.eat_char(':');
        self.skip_inline_ws();
        // Header must terminate at EOL.
        match self.peek_byte() {
            Some(b'\n') | Some(b'\r') | None => {}
            Some(_other) => return Err(self.err("unexpected content after radar-beta".into())),
        }
        Ok(())
    }

    fn parse_axis_list(&mut self) -> Result<Vec<RadarAxis>> {
        let mut out = Vec::new();
        loop {
            self.skip_inline_ws();
            let name = self.parse_ident()?;
            let label = self.parse_opt_label()?.unwrap_or_else(|| name.clone());
            out.push(RadarAxis { name, label });
            self.skip_inline_ws();
            if self.peek_byte() == Some(b',') {
                self.bump();
                self.skip_blank_lines();
                continue;
            }
            self.finish_line()?;
            break;
        }
        Ok(out)
    }

    fn parse_curve_list(&mut self, axes: &[RadarAxis]) -> Result<Vec<RadarCurve>> {
        let mut out = Vec::new();
        loop {
            self.skip_inline_ws();
            let name = self.parse_ident()?;
            let label = self.parse_opt_label()?.unwrap_or_else(|| name.clone());
            self.skip_inline_ws();
            if !self.eat_char('{') {
                return Err(self.err("expected '{' after curve name".into()));
            }
            let values = self.parse_entries(axes)?;
            self.skip_inline_ws();
            if !self.eat_char('}') {
                return Err(self.err("expected '}' to close curve entries".into()));
            }
            out.push(RadarCurve { label, values });
            self.skip_inline_ws();
            if self.peek_byte() == Some(b',') {
                self.bump();
                self.skip_blank_lines();
                continue;
            }
            self.finish_line()?;
            break;
        }
        Ok(out)
    }

    /// Parse entries inside `{ ... }`. Supports two alternatives:
    ///   (1) bare numbers, comma-separated.
    ///   (2) axis-qualified entries `IDENT[:]? NUMBER`, comma-separated.
    ///
    /// If alternative 2 is used, we reorder the values to match `axes`
    /// (matching upstream `db.ts::computeCurveEntries`).
    fn parse_entries(&mut self, axes: &[RadarAxis]) -> Result<Vec<f64>> {
        // Skip any newlines / whitespace between `{` and first entry.
        self.skip_blank_lines();

        enum Entry {
            Bare(f64),
            Keyed(String, f64),
        }

        let mut entries: Vec<Entry> = Vec::new();
        loop {
            self.skip_blank_lines();
            if self.peek_byte() == Some(b'}') {
                break;
            }
            // Lookahead: is this an identifier (keyed) or a number?
            let save = self.pos;
            let save_line = self.line;
            let save_col = self.col;
            let first = self.peek_byte();
            let is_ident_start =
                matches!(first, Some(b) if b.is_ascii_alphabetic() || b == b'_');
            if is_ident_start {
                let name = self.parse_ident()?;
                self.skip_inline_ws();
                // Optional `:` separator
                self.eat_char(':');
                self.skip_inline_ws();
                let v = self.parse_number()?;
                entries.push(Entry::Keyed(name, v));
            } else {
                // Undo any partial state (shouldn't have moved here; just defensive).
                self.pos = save;
                self.line = save_line;
                self.col = save_col;
                let v = self.parse_number()?;
                entries.push(Entry::Bare(v));
            }
            self.skip_blank_lines();
            if self.peek_byte() == Some(b',') {
                self.bump();
                continue;
            }
            break;
        }

        // Determine form.
        let has_keyed = entries.iter().any(|e| matches!(e, Entry::Keyed(..)));
        let has_bare = entries.iter().any(|e| matches!(e, Entry::Bare(_)));
        if has_keyed && has_bare {
            return Err(self.err("mixed bare and keyed curve entries".into()));
        }
        if has_keyed {
            if axes.is_empty() {
                return Err(self.err(
                    "axes must be declared before keyed curve entries".into(),
                ));
            }
            let mut ordered: Vec<f64> = Vec::with_capacity(axes.len());
            for axis in axes {
                let v = entries
                    .iter()
                    .find_map(|e| match e {
                        Entry::Keyed(n, v) if n == &axis.name => Some(*v),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        self.err(format!("missing entry for axis {}", axis.label))
                    })?;
                ordered.push(v);
            }
            Ok(ordered)
        } else {
            Ok(entries
                .into_iter()
                .map(|e| match e {
                    Entry::Bare(v) => v,
                    Entry::Keyed(_, v) => v,
                })
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal() {
        let src = "radar-beta\n    title Best Radar Ever\n    axis A, B, C\n    curve c1{1, 2, 3}\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.meta.title.as_deref(), Some("Best Radar Ever"));
        assert_eq!(d.axes.len(), 3);
        assert_eq!(d.axes[0].name, "A");
        assert_eq!(d.axes[0].label, "A");
        assert_eq!(d.curves.len(), 1);
        assert_eq!(d.curves[0].label, "c1");
        assert_eq!(d.curves[0].values, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn parse_keyed_entries_reorder() {
        let src = "radar-beta\naxis A, B, C\ncurve k{B 5, A 1, C 3}\n";
        let d = parse(src).expect("parse");
        // Values reordered to [A, B, C].
        assert_eq!(d.curves[0].values, vec![1.0, 5.0, 3.0]);
    }

    #[test]
    fn parse_options() {
        let src = "radar-beta\naxis A,B\ncurve c{1,2}\nshowLegend false\nticks 3\nmax 10\nmin -1\ngraticule polygon\n";
        let d = parse(src).expect("parse");
        assert!(!d.options.show_legend);
        assert_eq!(d.options.ticks, 3);
        assert_eq!(d.options.max, Some(10.0));
        assert_eq!(d.options.min, -1.0);
        assert_eq!(d.options.graticule, Graticule::Polygon);
    }

    #[test]
    fn parse_labeled_axis() {
        let src = "radar-beta\naxis Stam[\"Stamina\"]\ncurve c{1}\n";
        let d = parse(src).expect("parse");
        assert_eq!(d.axes[0].name, "Stam");
        assert_eq!(d.axes[0].label, "Stamina");
    }
}
