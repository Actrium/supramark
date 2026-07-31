//! GFM autolink extension — bare `www.`, scheme (`http://`/`https://`/`ftp://`)
//! and email autolinking, ported from cmark-gfm 0.29's `extensions/autolink.c`
//! so trailing-punctuation, paren-balancing, `<` truncation, `&entity;`
//! truncation and `mailto:`/`xmpp:` rewind all match GitHub's output exactly.
//!
//! Unlike the CommonMark `<url>` autolink (a separate inline rule), the GFM
//! extension runs as a postprocess over already-parsed text nodes: emphasis,
//! images and other inline constructs apply first, then URLs inside the
//! resulting text are linkified (matching cmark-gfm's own postprocess pass).
use crate::common::sourcemap::SourcePos;
use crate::parser::core::CoreRule;
use crate::parser::inline::{Text, TextSpecial};
use crate::supramark::{AstV2Ctx, SupramarkNode};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct GfmAutolink {
    pub url: String,
}

impl NodeValue for GfmAutolink {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &AstV2Ctx<'_>,
    ) -> Option<Vec<SupramarkNode>> {
        Some(vec![SupramarkNode::Link {
            url: self.url.clone(),
            title: None,
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("href", self.url.clone()));
        fmt.open("a", &attrs);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.add_rule::<GfmAutolinkPostprocess>().after_all();
}

#[doc(hidden)]
pub struct GfmAutolinkPostprocess;
impl CoreRule for GfmAutolinkPostprocess {
    fn run(root: &mut Node, md: &MarkdownParser) {
        process_node(root, false, md);
    }
}

/// Walk the tree, linkifying text nodes that are not inside a link/image.
fn process_node(node: &mut Node, in_link: bool, md: &MarkdownParser) {
    let is_link = node.name().ends_with("Link") || node.name().ends_with("Autolink");
    // Image alt text is collected as a plain string attribute, so URLs inside
    // it must stay literal — cmark-gfm does not linkify inside images either.
    let is_image = node.name().ends_with("::Image");
    let child_in_link = in_link || is_link || is_image;
    let mut i = 0;
    while i < node.children.len() {
        // Code spans/blocks hold their content as text children; cmark-gfm's
        // autolink postprocess only walks CMARK_NODE_TEXT, so it never enters a
        // code node. Mirror that here to keep bare URLs inside `...` literal.
        let is_code = is_code_node(&node.children[i]);
        if !child_in_link && !is_code {
            // Borrow the child once for both the cast and the srcmap; the
            // values we need (content clone, the Copy srcmap) outlive the
            // borrow, so splice below is fine.
            let orig_srcmap = node.children[i].srcmap;
            if let Some(text) = node.children[i].cast::<Text>() {
                let content = text.content.clone();
                if let Some(splice) = autolink_split(&content, orig_srcmap, md) {
                    node.children.splice(i..=i, splice);
                    // Re-scan from the new node at index i (the trailing text
                    // fragment, if any) so consecutive URLs are linkified.
                    continue;
                }
            }
        }
        if !is_code {
            process_node(&mut node.children[i], child_in_link, md);
        }
        i += 1;
    }
}

fn is_code_node(node: &Node) -> bool {
    let name = node.name();
    name.ends_with("CodeInline") || name.ends_with("CodeBlock") || name.ends_with("CodeFence")
}

/// Outcome of scanning one text node: a list of replacement nodes (text +
/// link fragments) that splice in place of the original.
///
/// `base_start` is the original text node's source byte offset (start of its
/// `srcmap`), if any. When present, each split fragment and autolink node
/// carries a `srcmap` derived as `base_start + fragment_byte_range`, so
/// source positions survive the split instead of all dropping to `None`.
/// The text node's content is a contiguous run of the source
/// (`srcmap.end - srcmap.start == content.len()`), so byte offsets in
/// `content` map 1:1 onto source byte offsets — the same invariant
/// `map_inline_text` relies on.
fn autolink_split(
    content: &str,
    orig_srcmap: Option<SourcePos>,
    md: &MarkdownParser,
) -> Option<Vec<Node>> {
    let bytes = content.as_bytes();
    let base_start = orig_srcmap.map(|p| p.get_byte_offsets().0);
    // For a content slice [lo..hi], the source span [base_start+lo, base_start+hi).
    let pos = |lo: usize, hi: usize| -> Option<SourcePos> {
        base_start.map(|b| SourcePos::new(b + lo, b + hi))
    };
    let mut out: Vec<Node> = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let m = match bytes[i] {
            b'@' => match email_match(bytes, i) {
                EmailScan::Found(m) => Some(m),
                // No email here; jump past the scanned region so the cursor
                // does not re-trigger email_match at the next '@' (the
                // quadratic case). Text in the skipped span is still flushed
                // — either by the next match's `text_start..m.start` slice
                // or by the trailing flush below — because text_start only
                // advances on a real match.
                EmailScan::Skip(skip) => {
                    i += skip;
                    continue;
                }
            },
            _ => match_next(bytes, i),
        };
        let Some(m) = m else {
            i += 1;
            continue;
        };
        // flush text before the match
        if m.start > text_start {
            push_text(&mut out, &content[text_start..m.start], pos(text_start, m.start));
        } else if m.start < text_start {
            // match rewinds into already-flushed text — shouldn't happen, but
            // bail to avoid corrupting the tree.
            return None;
        }
        push_link(&mut out, &m.url, &m.display, pos(m.start, m.end), md);
        text_start = m.end;
        i = m.end;
    }
    if out.is_empty() {
        return None;
    }
    if text_start < bytes.len() {
        push_text(&mut out, &content[text_start..], pos(text_start, bytes.len()));
    }
    // Drop empty trailing/leading text nodes that cmark-gfm would also drop.
    out.retain(|n| !(n.cast::<Text>().is_some_and(|t| t.content.is_empty())));
    Some(out)
}

struct Match {
    start: usize,
    end: usize,
    url: String,
    display: String,
}

/// Outcome of an email scan at one `@`.
///
/// Mirrors cmark-gfm's `postprocess_text` email branch: on a failed match the
/// outer cursor must advance past the scanned region (the `@` plus the
/// forward-scanned link_end, or just past the `@` when there is no local
/// part). Returning that distance here — instead of `None` — is what keeps
/// the outer `autolink_split` cursor from re-triggering `email_match` at
/// every subsequent `@`, which was O(n^2) on inputs like `a@a@a@...`.
enum EmailScan {
    Found(Match),
    /// Advance the outer cursor by this many bytes from the `@` position.
    Skip(usize),
}

fn push_text(out: &mut Vec<Node>, s: &str, pos: Option<SourcePos>) {
    let mut node = Node::new(Text {
        content: s.to_owned(),
    });
    node.srcmap = pos;
    out.push(node);
}

fn push_link(
    out: &mut Vec<Node>,
    url: &str,
    display: &str,
    pos: Option<SourcePos>,
    md: &MarkdownParser,
) {
    let full_url = md.link_formatter.normalize_link(url);
    if md.link_formatter.validate_link(&full_url).is_none() {
        // Disallowed protocol: render as plain text instead of a link.
        push_text(out, display, pos);
        return;
    }
    let mut inner = Node::new(TextSpecial {
        content: display.to_owned(),
        markup: display.to_owned(),
        info: "autolink",
    });
    inner.srcmap = pos;
    let mut node = Node::new(GfmAutolink {
        url: full_url,
    });
    node.srcmap = pos;
    node.children.push(inner);
    out.push(node);
}

fn match_next(bytes: &[u8], i: usize) -> Option<Match> {
    // cmark-gfm triggers www at 'w', url at ':' (scheme://), email at '@'.
    let b = bytes[i];
    if b == b'w' && bytes.get(i..i + 4) == Some(b"www.") {
        return www_match(bytes, i);
    }
    if b == b':' && bytes.get(i + 1) == Some(&b'/') && bytes.get(i + 2) == Some(&b'/') {
        return url_match(bytes, i);
    }
    // email at '@' is handled inline by autolink_split so the cursor can
    // consume the skip-distance returned by email_match.
    None
}

// ---- www ---------------------------------------------------------------

fn www_match(data: &[u8], www_pos: usize) -> Option<Match> {
    // preceding char must be whitespace, start-of-string, or one of *_~(
    if www_pos > 0 {
        let prev = data[www_pos - 1];
        let ok = is_space(prev) || matches!(prev, b'*' | b'_' | b'~' | b'(');
        if !ok {
            return None;
        }
    }
    let rest = &data[www_pos..];
    if rest.len() < 4 || &rest[..4] != b"www." {
        return None;
    }
    let domain_end = check_domain(rest, false)?;
    let mut link_end = domain_end;
    while link_end < rest.len() && !is_space(rest[link_end]) && rest[link_end] != b'<' {
        link_end += 1;
    }
    link_end = autolink_delim(rest, link_end);
    if link_end == 0 {
        return None;
    }
    let matched = std::str::from_utf8(&rest[..link_end]).ok()?;
    let url = format!("http://{}", matched);
    Some(Match {
        start: www_pos,
        end: www_pos + link_end,
        url,
        display: matched.to_owned(),
    })
}

// ---- scheme url ---------------------------------------------------------

fn url_match(data: &[u8], colon_pos: usize) -> Option<Match> {
    // rewind alpha scheme chars before ':'
    let mut rewind = 0usize;
    while colon_pos > rewind {
        let c = data[colon_pos - rewind - 1];
        if c.is_ascii_alphabetic() {
            rewind += 1;
        } else {
            break;
        }
    }
    if rewind == 0 {
        return None;
    }
    let scheme_start = colon_pos - rewind;
    let scheme = &data[scheme_start..colon_pos];
    if !is_safe_scheme(scheme) {
        return None;
    }
    let after = &data[colon_pos..];
    // cmark-gfm requires a valid host char immediately after "://".
    if after.len() <= 3 || !is_valid_hostchar(after[3]) {
        return None;
    }
    // after begins with "://"
    let mut link_end = 3; // "://"
    let domain_part = &after[link_end..];
    let domain_len = check_domain(domain_part, true)?;
    link_end += domain_len;
    while link_end < after.len() && !is_space(after[link_end]) && after[link_end] != b'<' {
        link_end += 1;
    }
    link_end = autolink_delim(after, link_end);
    if link_end == 0 {
        return None;
    }
    let matched = std::str::from_utf8(&data[scheme_start..colon_pos + link_end]).ok()?;
    Some(Match {
        start: scheme_start,
        end: colon_pos + link_end,
        url: matched.to_owned(),
        display: matched.to_owned(),
    })
}

fn is_safe_scheme(scheme: &[u8]) -> bool {
    scheme.eq_ignore_ascii_case(b"http")
        || scheme.eq_ignore_ascii_case(b"https")
        || scheme.eq_ignore_ascii_case(b"ftp")
}

// ---- email (port of postprocess_text) ----------------------------------
//
// cmark-gfm's `postprocess_text` (extensions/autolink.c) finds each `@` with
// memchr, then runs a `found_at:` rewind + forward scan. When the forward scan
// hits a second `@`, it does `offset += max_rewind + 1; max_rewind = link_end - 1;
// goto found_at` — rebasing to the later `@` *without* rescanning the bytes
// between them, and re-running the rewind for the new `@`. A single attempt
// thus walks the whole `@`-chain in linear time, and on failure advances
// `offset` past the scanned region so the outer loop does not re-trigger at
// every `@`. The recursion this was ported as re-scanned via the outer loop,
// turning `a@a@a@...` into O(n^2). This version restores the loop + skip.

fn email_match(data: &[u8], at_pos0: usize) -> EmailScan {
    let orig = at_pos0;
    let mut at_pos = at_pos0;
    let mut rewind_floor = 0usize;
    // Reset once per fresh `@` (cmark's `while(true)` body declarations).
    // Rebases (cmark's `goto found_at`) do NOT reset these — `auto_mailto`
    // can only flip true→false across a rebase, exactly as in cmark.
    let mut auto_mailto = true;
    let mut is_xmpp = false;
    loop {
        let max_rewind = at_pos - rewind_floor;
        let mut rewind = 0usize;
        while rewind < max_rewind {
            let c = data[at_pos - rewind - 1];
            if c.is_ascii_alphanumeric() {
                rewind += 1;
                continue;
            }
            if matches!(c, b'.' | b'+' | b'-' | b'_') {
                rewind += 1;
                continue;
            }
            if c == b':' {
                if validate_protocol(b"mailto:", data, at_pos, rewind, max_rewind) {
                    auto_mailto = false;
                    rewind += 1;
                    continue;
                }
                if validate_protocol(b"xmpp:", data, at_pos, rewind, max_rewind) {
                    auto_mailto = false;
                    is_xmpp = true;
                    rewind += 1;
                    continue;
                }
            }
            break;
        }
        if rewind == 0 {
            // No local-part char before '@': cmark does `offset += max_rewind + 1`,
            // i.e. advance one byte past the '@'.
            return EmailScan::Skip(at_pos - orig + 1);
        }

        // forward scan from after '@'
        let mut np = 0usize;
        let mut link_end = 1usize; // skip '@'
        let after_at = &data[at_pos..];
        let mut rebased = false;
        while link_end < after_at.len() {
            let c = after_at[link_end];
            if c.is_ascii_alphanumeric() {
                link_end += 1;
                continue;
            }
            if c == b'@' {
                // Second '@': rebase (cmark's `goto found_at`). The rewind
                // floor becomes the char right after the current '@', so the
                // new `max_rewind = link_end - 1` and the local part of the
                // later '@' cannot rewind past the current one.
                rewind_floor = at_pos + 1;
                at_pos += link_end;
                rebased = true;
                break;
            }
            if c == b'.'
                && link_end < after_at.len() - 1
                && after_at[link_end + 1].is_ascii_alphanumeric()
            {
                np += 1;
                link_end += 1;
                continue;
            }
            if c == b'/' && is_xmpp {
                link_end += 1;
                continue;
            }
            if c == b'-' || c == b'_' {
                link_end += 1;
                continue;
            }
            break;
        }
        if rebased {
            continue;
        }

        if link_end < 2 || np == 0 {
            // cmark: `offset += max_rewind + link_end` — advance past the
            // whole forward-scanned region.
            return EmailScan::Skip(at_pos - orig + link_end);
        }
        // last char of domain must be alpha or '.'
        let last = after_at[link_end - 1];
        if !(last.is_ascii_alphabetic() || last == b'.') {
            return EmailScan::Skip(at_pos - orig + link_end);
        }

        link_end = autolink_delim(after_at, link_end);
        if link_end == 0 {
            // cmark: `offset += max_rewind + 1` — advance one past the '@'.
            return EmailScan::Skip(at_pos - orig + 1);
        }

        let local_and_domain = &data[at_pos - rewind..at_pos + link_end];
        let matched = match std::str::from_utf8(local_and_domain) {
            Ok(s) => s,
            Err(_) => return EmailScan::Skip(at_pos - orig + link_end),
        };
        let url = if auto_mailto {
            format!("mailto:{}", matched)
        } else {
            matched.to_owned()
        };
        return EmailScan::Found(Match {
            start: at_pos - rewind,
            end: at_pos + link_end,
            url,
            display: matched.to_owned(),
        });
    }
}

fn validate_protocol(
    protocol: &[u8],
    data: &[u8],
    at_pos: usize,
    rewind: usize,
    max_rewind: usize,
) -> bool {
    let len = protocol.len();
    if len > max_rewind - rewind {
        return false;
    }
    let proto_start = at_pos - rewind - len;
    if &data[proto_start..proto_start + len] != protocol {
        return false;
    }
    if len == max_rewind - rewind {
        return true;
    }
    // char before protocol must be non-alphanumeric
    let prev = data[proto_start - 1];
    !prev.is_ascii_alphanumeric()
}

// ---- shared helpers (ported from cmark-gfm) -----------------------------

fn autolink_delim(data: &[u8], mut link_end: usize) -> usize {
    let mut opening = 0usize;
    let mut closing = 0usize;
    for i in 0..link_end {
        let c = data[i];
        if c == b'<' {
            link_end = i;
            break;
        } else if c == b'(' {
            opening += 1;
        } else if c == b')' {
            closing += 1;
        }
    }
    while link_end > 0 {
        match data[link_end - 1] {
            b')' => {
                if closing <= opening {
                    return link_end;
                }
                closing -= 1;
                link_end -= 1;
            }
            b'?' | b'!' | b'.' | b',' | b':' | b'*' | b'_' | b'~' | b'\'' | b'"' => {
                link_end -= 1;
            }
            b';' => {
                // cmark-gfm 0.29 autolink.c: `new_end = link_end - 2` then scan
                // back over alphas for a `&entity;` tail. The C version relies
                // on link_end >= 2 (a `;` at offset 0/1 cannot be a valid URL
                // end) and would underflow on `size_t` otherwise. Guard the
                // entity scan so the port cannot panic on a degenerate input;
                // when there is no room for `&x;`, fall through to the
                // single-char trim (`link_end -= 1`) exactly as the else arm.
                if link_end >= 2 {
                    let tail = link_end - 2;
                    let mut new_end = tail;
                    while new_end > 0 && data[new_end].is_ascii_alphabetic() {
                        new_end -= 1;
                    }
                    if new_end < tail && data[new_end] == b'&' {
                        link_end = new_end;
                    } else {
                        link_end -= 1;
                    }
                } else {
                    link_end -= 1;
                }
            }
            _ => return link_end,
        }
    }
    link_end
}

fn check_domain(data: &[u8], allow_short: bool) -> Option<usize> {
    let mut np = 0usize;
    let mut uscore1 = 0usize;
    let mut uscore2 = 0usize;
    let size = data.len();
    let mut i = 1usize;
    while i < size - 1 {
        // Mirror cmark-gfm 0.29.0.gfm.13 autolink.c check_domain: when a
        // backslash is followed by at least one more byte, skip the backslash
        // itself and classify the *next* byte. The previous port captured `c`
        // before this increment, so the backslash was tested against
        // is_valid_hostchar and broke the domain — diverging from cmark, which
        // linkifies `www.foo\.bar` (href percent-encodes the backslash).
        if data[i] == b'\\' && i < size - 2 {
            i += 1;
        }
        let c = data[i];
        if c == b'_' {
            uscore2 += 1;
        } else if c == b'.' {
            uscore1 = uscore2;
            uscore2 = 0;
            np += 1;
        } else if !is_valid_hostchar(c) && c != b'-' {
            break;
        }
        i += 1;
    }
    if uscore1 > 0 || uscore2 > 0 {
        if np <= 10 {
            return None;
        }
    }
    if allow_short {
        Some(i)
    } else {
        if np > 0 {
            Some(i)
        } else {
            None
        }
    }
}

fn is_valid_hostchar(b: u8) -> bool {
    if b < 0x80 {
        return !is_space(b) && !is_punct(b);
    }
    // Non-ASCII lead byte. cmark-gfm 0.29.0.gfm.13 autolink.c is_valid_hostchar
    // calls cmark_utf8proc_iterate to decode the full codepoint, then rejects
    // cmark_utf8proc_is_space / cmark_utf8proc_is_punctuation codepoints. This
    // port takes only the lead byte: continuation bytes (0x80-0xBF) as a start
    // are invalid UTF-8 -> not valid (matches cmark_utf8proc_iterate's error);
    // lead bytes (>=0xC0) are accepted unconditionally. That is broader than
    // cmark for non-ASCII *punctuation* (e.g. U+2018/201C/2026), which cmark
    // would reject and break the domain on. The conformance suite does not
    // exercise non-ASCII host characters, and faithfully porting cmark's
    // curated Unicode punctuation table (~80 lines of codepoints in utf8.c) is
    // out of scope for this change; left as a documented edge divergence.
    b >= 0xC0
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn is_punct(b: u8) -> bool {
    matches!(
        b,
        b'!' | b'"'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'('
            | b')'
            | b'*'
            | b'+'
            | b','
            | b'-'
            | b'.'
            | b'/'
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'@'
            | b'['
            | b'\\'
            | b']'
            | b'^'
            | b'_'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The matchers are unit-tested directly; the full HTML pipeline is
    // exercised end-to-end by the cmark-gfm conformance suite (spec-0621..0631,
    // extensions-0019) and by public_api.rs.

    #[test]
    fn www_basic() {
        let m = www_match(b"www.commonmark.org", 0).unwrap();
        assert_eq!(m.url, "http://www.commonmark.org");
        assert_eq!(m.display, "www.commonmark.org");
        assert_eq!(m.end, 18);
    }

    #[test]
    fn www_trailing_dot_stripped() {
        // "www.commonmark.org." -> link "www.commonmark.org", "." stays
        let m = www_match(b"www.commonmark.org.", 0).unwrap();
        assert_eq!(m.display, "www.commonmark.org");
        assert_eq!(m.end, 18);
    }

    #[test]
    fn www_paren_balanced() {
        let m = www_match(b"www.google.com/search?q=Markup+(business)", 0).unwrap();
        assert_eq!(m.display, "www.google.com/search?q=Markup+(business)");
    }

    #[test]
    fn www_lt_truncates() {
        let m = www_match(b"www.commonmark.org/he<lp", 0).unwrap();
        assert_eq!(m.display, "www.commonmark.org/he");
        assert_eq!(m.end, 21); // stops before '<'
    }

    #[test]
    fn www_entity_truncates() {
        let m = www_match(b"www.google.com/search?q=commonmark&hl;", 0).unwrap();
        assert_eq!(m.display, "www.google.com/search?q=commonmark");
    }

    #[test]
    fn www_backslash_in_domain_is_skipped_not_break() {
        // cmark-gfm 0.29.0.gfm.13 linkifies `www.foo\.bar`: check_domain skips
        // the backslash and classifies the next byte, so the domain extends
        // past it (the href serializer percent-encodes the backslash to %5C at
        // render time; the matcher itself keeps the raw byte). The previous
        // port captured the byte before the backslash-skip and broke the
        // domain on the backslash itself, truncating the link to `www.foo`.
        let m = www_match(b"www.foo\\.bar", 0).unwrap();
        assert_eq!(m.display, "www.foo\\.bar");
        assert_eq!(m.url, "http://www.foo\\.bar");
    }

    #[test]
    fn www_preceding_paren_ok() {
        // "(www.google.com)" -> preceding '(' is allowed
        let m = www_match(b"(www.google.com/search?q=Markup+(business))", 1).unwrap();
        assert_eq!(m.display, "www.google.com/search?q=Markup+(business)");
    }

    #[test]
    fn www_preceding_word_rejected() {
        assert!(www_match(b"foo.www.commonmark.org", 4).is_none());
    }

    #[test]
    fn url_http() {
        let m = url_match(b"http://commonmark.org", 4).unwrap();
        assert_eq!(m.url, "http://commonmark.org");
        assert_eq!(m.start, 0);
    }

    #[test]
    fn url_paren_in_path() {
        let m = url_match(b"https://encrypted.google.com/search?q=Markup+(business)", 5).unwrap();
        assert_eq!(m.url, "https://encrypted.google.com/search?q=Markup+(business)");
    }

    #[test]
    fn email_basic() {
        let EmailScan::Found(m) = email_match(b"foo@bar.baz", 3) else {
            panic!("expected match");
        };
        assert_eq!(m.url, "mailto:foo@bar.baz");
        assert_eq!(m.display, "foo@bar.baz");
    }

    #[test]
    fn email_trailing_dot_stripped() {
        let EmailScan::Found(m) = email_match(b"a.b-c_d@a.b.", 7) else {
            panic!("expected match");
        };
        assert_eq!(m.display, "a.b-c_d@a.b");
    }

    #[test]
    fn email_trailing_dash_rejected() {
        // "a.b-c_d@a.b-" -> last char '-' not alpha/'.' -> no match
        assert!(matches!(
            email_match(b"a.b-c_d@a.b-", 7),
            EmailScan::Skip(_)
        ));
    }

    #[test]
    fn email_mailto_prefix() {
        // "mailto:scyther@pokemon.com" -> '@' at index 14
        let EmailScan::Found(m) = email_match(b"mailto:scyther@pokemon.com", 14) else {
            panic!("expected match");
        };
        assert_eq!(m.url, "mailto:scyther@pokemon.com");
        assert_eq!(m.display, "mailto:scyther@pokemon.com");
    }

    #[test]
    fn email_xmpp_prefix() {
        // "xmpp:scyther@pokemon.com" -> '@' at index 12
        let EmailScan::Found(m) = email_match(b"xmpp:scyther@pokemon.com", 12) else {
            panic!("expected match");
        };
        assert!(m.url.starts_with("xmpp:"));
        assert_eq!(m.display, "xmpp:scyther@pokemon.com");
    }

    #[test]
    fn email_mmmmail_no_protocol() {
        // "mmmmailto:scyther@pokemon.com" -> char before "mailto:" is 'm' (alnum)
        // -> validate_protocol fails -> auto_mailto stays true -> only the bare
        // email is matched, "mmmmailto:" stays as text. '@' at index 17.
        let EmailScan::Found(m) = email_match(b"mmmmailto:scyther@pokemon.com", 17) else {
            panic!("expected match");
        };
        assert_eq!(m.url, "mailto:scyther@pokemon.com");
        assert_eq!(m.display, "scyther@pokemon.com");
    }

    #[test]
    fn email_rebase_to_last_at() {
        // `a@b@c.d`: the first '@' rebases to the second; the local part is
        // just `b` (cmark forbids rewinding past the char after the first
        // '@'), so the match is `b@c.d`. Verified against cmark-gfm 0.29.0.gfm.13.
        let EmailScan::Found(m) = email_match(b"a@b@c.d", 1) else {
            panic!("expected match");
        };
        assert_eq!(m.start, 2);
        assert_eq!(m.end, 7);
        assert_eq!(m.url, "mailto:b@c.d");
        assert_eq!(m.display, "b@c.d");
    }

    #[test]
    fn email_no_dot_skips_chain() {
        // `a@a@a@a` has no '.', so every '@' chain bottoms out at np==0.
        // email_match must return Skip covering the whole chain (not a
        // per-`@` None), which is what keeps the outer cursor O(n).
        assert!(matches!(
            email_match(b"a@a@a@a", 1),
            EmailScan::Skip(6)
        ));
    }

    #[test]
    fn email_chain_is_linear() {
        // Regression for the quadratic recursion: an `a@`-repeated input
        // must finish quickly. There is no email (no dot), so the whole
        // chain skips. We assert the skip distance equals the full span,
        // i.e. a single scan handled every '@'.
        let n = 4000;
        let mut input = String::with_capacity(2 * n);
        for _ in 0..n {
            input.push_str("a@");
        }
        input.push('a');
        let bytes = input.as_bytes();
        // First '@' at index 1; a single email_match should skip the lot.
        assert!(matches!(
            email_match(bytes, 1),
            EmailScan::Skip(s) if s == 2 * n
        ));
    }
}
