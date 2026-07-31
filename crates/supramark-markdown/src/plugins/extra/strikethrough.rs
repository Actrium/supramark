//! GFM strikethrough syntax (`~single~` and `~~double~~`).
//!
//! Ported from cmark-gfm 0.29's `extensions/strikethrough.c`: both `~` and `~~`
//! open/close a single `<del>` (no nesting by length), the opening and closing
//! runs must have **equal** length, and runs of 3+ tildes are literal text (not
//! delimiters). The standard emphasis flanking rules still apply; the rule-of-3
//! is a no-op here because the exact-length constraint already rejects the only
//! (1,2)/(2,1) pairs it could affect.
//!
//! This is a custom inline rule rather than `emph_pair::add_with`, because the
//! shared emphasis machinery splits runs and matches by `min(len)` — which would
//! turn `~~~a~~~` into nested `<del>`s and `~mismatch~~` into a partial match.
use crate::generics::inline::emph_pair::{EmphMarker, FragmentsJoin};
use crate::parser::core::CoreRule;
use crate::parser::inline::builtin::InlineParserRule;
use crate::parser::inline::{InlineRule, InlineState, Text};
use crate::{MarkdownParser, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Strikethrough {
    pub marker: char,
}

impl NodeValue for Strikethrough {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::Delete {
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("del", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("del");
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.inline.add_rule::<StrikethroughScanner>();
    // Convert any leftover (unmatched) `~` markers into plain text and merge
    // adjacent text fragments. Runs after inline parsing; the generic
    // FragmentsJoin also handles EmphMarker leftovers from other emphasis.
    if !md.has_rule::<FragmentsJoin>() {
        md.add_rule::<FragmentsJoin>()
            .before_all()
            .after::<InlineParserRule>();
    }
}

#[doc(hidden)]
pub struct StrikethroughScanner;
impl InlineRule for StrikethroughScanner {
    const MARKER: char = '~';

    fn check(_: &mut InlineState) -> Option<usize> {
        None
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        // The inline parser only invokes this rule when the byte at `state.pos`
        // is `~`, so the slice is non-empty and the first char is `~`; compare
        // against `Some('~')` instead of `.unwrap()` so a degenerate empty
        // slice cannot panic on user input.
        if chars.next() != Some('~') {
            return None;
        }

        let scanned = state.scan_delims(state.pos, true);

        // Runs of 3+ tildes, or runs that can neither open nor close, are plain
        // literal text (cmark-gfm only pushes a delimiter for len 1 or 2 with a
        // flanking side).
        let node = if scanned.length >= 3 || (!scanned.can_open && !scanned.can_close) {
            Node::new(Text {
                content: "~".repeat(scanned.length),
            })
        } else {
            let mut marker = Node::new(EmphMarker {
                marker: '~',
                length: scanned.length,
                remaining: scanned.length,
                open: scanned.can_open,
                close: scanned.can_close,
                content_start: state.pos,
                content_end: state.pos + scanned.length,
            });
            marker.srcmap = state.get_map(state.pos, state.pos + scanned.length);
            scan_and_match(state, marker)
        };

        Some((node, scanned.length))
    }
}

/// Assuming the just-scanned node is a potential closer, look backward through
/// the current children for a matching opener and, if found, wrap the
/// intervening content in a `Strikethrough` node.
fn scan_and_match(state: &mut InlineState, closer_node: Node) -> Node {
    let closer = match closer_node.cast::<EmphMarker>() {
        Some(m) => m.clone(),
        None => return closer_node,
    };
    if !closer.close {
        // Can't close — leave it as a marker; FragmentsJoin turns it into text.
        return closer_node;
    }

    let mut idx = state.node.children.len();
    while idx > 0 {
        idx -= 1;
        let opener = match state.node.children[idx].cast::<EmphMarker>() {
            Some(m) => m.clone(),
            None => continue,
        };
        if !opener.open || opener.marker != '~' || opener.length != closer.length {
            // GFM requires an exact length match; keep scanning backward.
            continue;
        }

        // Match: move everything after the opener into a <del> node.
        let mut del = Node::new(Strikethrough { marker: '~' });
        del.children = state.node.children.split_off(idx + 1);
        // The opener marker is now the last child — drop it.
        state.node.children.pop();
        state.node.children.push(del);

        // The closer is fully consumed. Pop the just-pushed <del> node and
        // return it so the caller re-pushes it (net no-op), and the consumed
        // closer is dropped — mirroring `emph_pair`'s contract.
        return state.node.children.pop().unwrap();
    }

    // No matching opener: leave the closer as a marker for later text conversion.
    closer_node
}
