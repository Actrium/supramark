//! WikiLink extension: `[[target]]`, `[[target|label]]`, `[[target#section]]`.
//!
//! Knowledge-base Markdown (Obsidian, Logseq) commonly uses WikiLinks. The
//! scanner recognizes `[[` … first `]]` and splits the content on the first
//! `|` (display label) and, inside the target part only, the first `#`
//! (heading fragment). `[[#section]]` (same-page link) keeps an empty target
//! with the section set.
//!
//! Degradation contract: any form that is not a well-formed WikiLink —
//! unclosed `[[foo`, a `[` or a line break inside the content, a single `]`
//! not immediately followed by `]`, or an empty required part (`[[]]`,
//! `[[target|]]`, `[[target#]]`, `[[|label]]`) — is left to the CommonMark
//! rules and ends up as literal text (or a CommonMark link, e.g. when a
//! `[target]: url` reference definition exists and the option is off). No
//! diagnostic is emitted: ordinary prose may contain `[[`, so a warning would
//! misfire; the degradation is the documented behavior, not a parse error.
//!
//! Backslash escapes inside the content are NOT interpreted (a `\` is a
//! literal character, matching Obsidian). Escaping the *opening* brackets
//! (`\[[foo]]`) still works because the CommonMark escape rule consumes `\[`
//! before this scanner runs. Code spans/fences can never contain a WikiLink
//! because the backtick rule consumes those bytes first.
//!
//! Inline math (`$…$`) wins over WikiLink: `$[[foo]]$` stays one
//! `math_inline` node. Math is claimed by the text post-pass
//! (`supramark::map_inline_text`), which runs per text node after
//! tokenization — if this scanner consumed the `[[`, the text run would split
//! and the span would be lost. So the scanner defers to math semantics: it
//! declines the `[[` when the current text run ends inside an open math span
//! (a dangling `$` with no same-line closer inside the run) and an unescaped
//! same-line `$` follows the `]]`. Degradation direction: when an inline
//! token (e.g. emphasis) sits between the `]]` and the closing `$`, the
//! post-pass would not claim the span but this scanner still declines, so
//! the `[[…]]` renders as literal text instead of a WikiLink.
//!
//! Resolution to a file path or URL is intentionally not done here: the AST
//! node carries the raw `target`/`section`/`label` and downstream hosts apply
//! their own workspace rules.
use crate::generics::inline::full_link::LinkScanner;
use crate::parser::inline::{InlineRule, InlineState, Text};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct WikiLink {
    pub target: String,
    pub section: Option<String>,
    pub label: Option<String>,
}

impl NodeValue for WikiLink {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::WikiLink {
            target: self.target.clone(),
            section: self.section.clone(),
            label: self.label.clone(),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        // The legacy HTML renderer has no workspace resolver, so the href is
        // the raw target; AST v2 consumers resolve via their own host rules.
        let mut attrs = node.attrs.clone();
        attrs.push(("href", self.target.clone()));
        fmt.open("a", &attrs);
        let text = match (&self.label, &self.section) {
            (Some(label), _) => label.clone(),
            (None, Some(section)) => format!("{} > {}", self.target, section),
            (None, None) => self.target.clone(),
        };
        fmt.text(&text);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownParser) {
    // Before LinkScanner so `[[target]]` is a WikiLink even when a
    // `[target]: url` reference definition exists — wiki semantics win.
    md.inline
        .add_rule::<WikiLinkScanner>()
        .before::<LinkScanner<false>>();
}

#[doc(hidden)]
pub struct WikiLinkScanner;
impl InlineRule for WikiLinkScanner {
    const MARKER: char = '[';

    /// MUST return `None` unconditionally.
    ///
    /// `check` is the validation-mode entry point used by `skip_token`, which
    /// `parse_link_label` relies on for bracket counting: it treats a `[` as
    /// one more nesting level only when `prev_pos == state.pos - 1` (a
    /// single-char skip). If this returned the multi-char WikiLink length,
    /// `[text [[x]] text](url)` would abort the label scan under
    /// `!enable_nested` and stop being a CommonMark link whenever the option
    /// is on. Returning `None` keeps validation-mode consumption byte-identical;
    /// the label's inline content is re-tokenized afterwards, where `run`
    /// produces the WikiLink child.
    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let src = state.src.as_bytes();
        let pos = state.pos;
        let max = state.pos_max;
        if pos + 1 >= max || src[pos] != b'[' || src[pos + 1] != b'[' {
            return None;
        }

        // Scan for the first `]]`. `[` or a line break aborts (nested or
        // multi-line content is not a WikiLink); a lone `]` not immediately
        // followed by another `]` aborts. All delimiters are ASCII, so byte
        // comparison is safe; `char_indices` advances whole codepoints.
        let mut close = None;
        for (offset, ch) in state.src[pos + 2..max].char_indices() {
            match ch {
                '\n' | '\r' | '[' => return None,
                ']' => {
                    let next = state.src[pos + 2 + offset + 1..max].chars().next();
                    if next == Some(']') {
                        close = Some(pos + 2 + offset);
                        break;
                    }
                    return None;
                }
                _ => {}
            }
        }
        let close = close?;

        let content = &state.src[pos + 2..close];
        let (target_part, label) = match content.split_once('|') {
            Some((target_part, label)) if !label.is_empty() => (target_part, Some(label)),
            // `[[target|]]` — empty display label degrades.
            Some(_) => return None,
            None => (content, None),
        };
        let (target, section) = match target_part.split_once('#') {
            Some((target, section)) if !section.is_empty() => (target, Some(section)),
            // `[[target#]]` / `[[#]]` — empty fragment degrades.
            Some(_) => return None,
            None => (target_part, None),
        };
        // `[[]]` / `[[|label]]` — a WikiLink needs a target or a fragment.
        if target.is_empty() && section.is_none() {
            return None;
        }

        if inside_math_span(state, close) {
            return None;
        }

        let node = Node::new(WikiLink {
            target: target.to_owned(),
            section: section.map(str::to_owned),
            label: label.map(str::to_owned),
        });
        Some((node, close + 2 - pos))
    }
}

/// True when the `[[` sits inside what the inline-math text post-pass would
/// claim as a `$…$` span (see the module docs). Two conditions must hold:
/// the current text run ends with a dangling math opener, and a same-line
/// unescaped `$` follows the `]]` so the span actually closes.
fn inside_math_span(state: &InlineState, close: usize) -> bool {
    ends_inside_open_math_span(state) && math_closer_follows(state, close + 2)
}

/// Simulates the math post-pass pairing (`supramark::find_next_inline_extension`)
/// over the current text run — the trailing `Text` child, whose content is raw
/// source bytes ending exactly at `state.pos`. Returns true when the run ends
/// with a dangling `$` opener (no same-line closer inside the run).
///
/// Text runs never contain `\` (the escape rule consumes backslashes into
/// separate TextSpecial nodes), so every `$` in the run is unescaped.
fn ends_inside_open_math_span(state: &InlineState) -> bool {
    let Some(text) = state.node.children.last().and_then(|n| n.cast::<Text>()) else {
        return false;
    };
    let bytes = text.content.as_bytes();
    let mut i = 0;
    while let Some(p) = (i..bytes.len()).find(|&k| bytes[k] == b'$') {
        // Closer: the next `$` on the same line, mirroring
        // find_closing_math_delimiter's no-newline rule. The search may run
        // off the end of the run — in the full text value the run continues
        // past the `[[`, so the closer can live beyond it.
        let mut k = p + 1;
        loop {
            if k >= bytes.len() {
                // Run ended with the span still open.
                return true;
            }
            match bytes[k] {
                // A closer across a line break is never valid; the next `$`
                // after the newline is a fresh candidate.
                b'\n' => i = p + 1,
                // Non-empty span claimed; resume after the closer.
                b'$' if k > p + 1 => i = k + 1,
                // `$$` — empty content: the post-pass skips this opener and
                // retries from the second `$`.
                b'$' => i = p + 1,
                _ => {
                    k += 1;
                    continue;
                }
            }
            break;
        }
    }
    false
}

/// True when an unescaped `$` appears between `from` and the next line break
/// (or `pos_max`): a candidate closing delimiter for the open math span.
fn math_closer_follows(state: &InlineState, from: usize) -> bool {
    let bytes = state.src.as_bytes();
    let mut i = from;
    while i < state.pos_max && bytes[i] != b'\n' {
        if bytes[i] == b'$' && !is_escaped_byte(bytes, i) {
            return true;
        }
        i += 1;
    }
    false
}

fn is_escaped_byte(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut j = index;
    while j > 0 && bytes[j - 1] == b'\\' {
        backslashes += 1;
        j -= 1;
    }
    backslashes % 2 == 1
}
