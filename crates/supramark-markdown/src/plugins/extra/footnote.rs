//! Supramark footnote definitions: `[^label]: content` on its own line.
//!
//! Migrated from the document-level prescan to a block rule. Like blockquotes,
//! a definition is a container: following lines indented at least 4 columns
//! (relative to the definition's base) are absorbed as block children
//! (paragraph, blockquote, indented code, …), so multi-line definitions parse
//! the same way cmark-gfm parses them. The rule runs before the link-reference
//! rule so `[^a]:` is not mistaken for a link reference definition.
use crate::parser::block::{BlockRule, BlockState, LineOffset};
use crate::plugins::cmark::block::reference::ReferenceScanner;
use crate::{MarkdownParser, Node, NodeValue, Renderer};

/// Columns of indentation that qualify a line as footnote-definition
/// continuation (mirrors the CommonMark indented-code threshold).
const FOOTNOTE_DEF_INDENT: i32 = 4;

#[derive(Debug)]
pub struct FootnoteDef {
    pub label: String,
}

impl NodeValue for FootnoteDef {
    fn to_ast_v2(
        &self,
        node: &Node,
        ctx: &crate::supramark::AstV2Ctx<'_>,
    ) -> Option<Vec<crate::supramark::SupramarkNode>> {
        Some(vec![crate::supramark::SupramarkNode::FootnoteDefinition {
            index: 0,
            identifier: crate::supramark::normalize_footnote_identifier(&self.label),
            label: self.label.clone(),
            children: ctx.map_children(&node.children),
            position: ctx.position(node),
        }])
    }

    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.contents(&node.children);
    }
}

pub fn add(md: &mut MarkdownParser) {
    md.block
        .add_rule::<FootnoteDefScanner>()
        .before::<ReferenceScanner>();
}

#[doc(hidden)]
pub struct FootnoteDefScanner;

impl FootnoteDefScanner {
    /// Parse `[^label]:` at the start of a (indent-trimmed) line, returning the
    /// label and the byte column where the definition content begins.
    fn parse_header(line: &str) -> Option<(String, usize)> {
        let label_rest = line.strip_prefix("[^")?;
        let close = label_rest.find("]:")?;
        let label = &label_rest[..close];
        if label.is_empty() {
            return None;
        }
        let mut content_col = 2 + close + 2;
        let content_rest = line.get(content_col..)?;
        content_col += content_rest.len() - content_rest.trim_start().len();
        Some((label.to_owned(), content_col))
    }
}

impl BlockRule for FootnoteDefScanner {
    fn check(state: &mut BlockState) -> Option<()> {
        if state.line_indent(state.line) > 3 {
            return None;
        }
        Self::parse_header(state.get_line(state.line)).map(|_| ())
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line_indent(state.line) > 3 {
            return None;
        }
        let start_line = state.line;
        let line = state.get_line(start_line).to_owned();
        let (label, content_col) = Self::parse_header(&line)?;

        // Absolute byte offset where the definition content begins on the
        // header line (past `[^label]:` and any trailing spaces).
        let header_offsets_first_nonspace = state.line_offsets[start_line].first_nonspace;
        let content_offset = header_offsets_first_nonspace + content_col;

        // Save the original line offsets we are about to rewrite so they can be
        // restored after the nested tokenize call (mirrors the blockquote rule).
        let mut saved_offsets: Vec<LineOffset> = Vec::new();

        // Reposition the header line so the inner tokenizer sees the definition
        // content at the definition's base indent. If the header has no content
        // (`[^footnote]:`), this yields an empty line that the tokenizer skips.
        saved_offsets.push(state.line_offsets[start_line].clone());
        state.line_offsets[start_line].first_nonspace = content_offset;
        state.line_offsets[start_line].indent_nonspace = state.blk_indent as i32;

        // Scan forward over continuation lines that belong to this definition.
        // A line continues the definition when it is indented at least
        // `FOOTNOTE_DEF_INDENT` columns past the base. Blank lines inside the
        // definition are tolerated as block separators; a second consecutive
        // blank line ends the definition. A non-indented line that starts a new
        // block (another definition, heading, …) terminates it; otherwise a
        // non-indented line is a lazy paragraph continuation.
        let mut next_line = start_line + 1;
        let mut last_line_empty = false;

        while next_line < state.line_max {
            if state.line_indent(next_line) < 0 {
                // Outdented relative to a surrounding list item; not ours.
                break;
            }

            if state.is_empty(next_line) {
                if last_line_empty {
                    break;
                }
                last_line_empty = true;
                saved_offsets.push(state.line_offsets[next_line].clone());
                next_line += 1;
                continue;
            }

            if state.line_indent(next_line) >= FOOTNOTE_DEF_INDENT {
                saved_offsets.push(state.line_offsets[next_line].clone());
                // Strip the 4-column definition indent so the inner tokenizer
                // sees the continuation content at the right indentation. For
                // a 4-column indent `first_nonspace` already lands on the
                // content; deeper-indented lines are claimed by the indented
                // code rule (which keys off `indent_nonspace`, not
                // `first_nonspace`) before any rule that reads `get_line`.
                state.line_offsets[next_line].indent_nonspace -= FOOTNOTE_DEF_INDENT;
                last_line_empty = false;
                next_line += 1;
                continue;
            }

            // Indent 0..3: either a new block at the definition level, or a
            // lazy paragraph continuation of the definition's last paragraph.
            state.line = next_line;
            if state.test_rules_at_line() {
                state.line = start_line;
                break;
            }
            state.line = start_line;

            if last_line_empty {
                break;
            }
            saved_offsets.push(state.line_offsets[next_line].clone());
            state.line_offsets[next_line].indent_nonspace = -1;
            last_line_empty = false;
            next_line += 1;
        }

        // Parse the absorbed range as the definition's block children.
        let old_node = std::mem::replace(&mut state.node, Node::new(FootnoteDef { label }));
        let old_line_max = state.line_max;
        state.line = start_line;
        state.line_max = next_line;
        state.md.block.tokenize(state);
        let consumed = state.line - start_line;
        state.line = start_line;
        state.line_max = old_line_max;

        // Restore the original offsets.
        for (idx, mut offset) in saved_offsets.into_iter().enumerate() {
            std::mem::swap(&mut state.line_offsets[idx + start_line], &mut offset);
        }

        let node = std::mem::replace(&mut state.node, old_node);
        Some((node, consumed.max(1)))
    }
}
