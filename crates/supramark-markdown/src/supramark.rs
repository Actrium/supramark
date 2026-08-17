use crate::plugins::cmark::block::fence::CodeFence;
use crate::plugins::extra::tables::{ColumnAlignment, TableBody, TableCell, TableHead, TableRow};
use crate::{MarkdownParser, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePoint {
    pub line: u32,
    pub column: u32,
    pub byte_offset: usize,
    pub utf16_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourcePosition {
    pub start: SourcePoint,
    pub end: SourcePoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParserInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<SourcePosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TableAlign {
    Left,
    Right,
    Center,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionMode {
    Transparent,
    Opaque,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SupramarkNode {
    Root {
        ast_version: u8,
        children: Vec<SupramarkNode>,
        diagnostics: Vec<Diagnostic>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parser: Option<ParserInfo>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Paragraph {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Heading {
        depth: u8,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Text {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Strong {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Emphasis {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    InlineCode {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Link {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Image {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        alt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Break {
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Delete {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Code {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Diagram {
        engine: String,
        code: String,
        /// True only when the Markdown source contains an explicit closing fence.
        ///
        /// Defaults to `false` when absent so previously persisted ASTs (which
        /// predate this field) still deserialize instead of failing. Consumers
        /// treat a missing value as "potentially open" and defer engine work,
        /// which is the safe direction.
        #[serde(default)]
        fence_closed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<serde_json::Value>,
        /// Semantic AST envelope { engine, kind, data }. None = not parsed or unsupported
        /// (lazy by default; not inlined in the parser main path, filled in on demand by
        /// downstream). See docs/architecture/diagram-semantic-ast.zh.md.
        #[serde(skip_serializing_if = "Option::is_none")]
        semantic: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    List {
        ordered: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        start: Option<u32>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    ListItem {
        #[serde(skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Blockquote {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    ThematicBreak {
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Table {
        align: Vec<Option<TableAlign>>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    TableRow {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    TableCell {
        #[serde(skip_serializing_if = "Option::is_none")]
        align: Option<TableAlign>,
        header: bool,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    MathBlock {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    MathInline {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    DefinitionList {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    DefinitionItem {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    DefinitionTerm {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    DefinitionDescription {
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    FootnoteDefinition {
        index: u32,
        label: String,
        identifier: String,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    FootnoteReference {
        index: u32,
        label: String,
        identifier: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Container {
        name: String,
        mode: ExtensionMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<String>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Input {
        name: String,
        mode: ExtensionMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<String>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Raw {
        format: String,
        value: String,
        block: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    Unsupported {
        syntax: String,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        diagnostics: Vec<Diagnostic>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
}

/// Runtime parse options. Mirrors the subset of micromark's per-case options
/// that the conformance harness needs to forward (`disable`, `allowDangerousHtml`,
/// `allowDangerousProtocol`).
///
/// Defaults keep the parser's pre-option behaviour: raw HTML and dangerous
/// protocols pass through (equivalent to micromark with both `allowDangerous*`
/// enabled). The conformance harness selects the safe profile explicitly
/// per case; callers that want micromark's safe-by-default can set
/// `allow_dangerous_html: false` / `allow_dangerous_protocol: false`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseOptions {
    #[serde(default = "default_true")]
    pub gfm_tables: bool,
    #[serde(default = "default_true")]
    pub gfm_strikethrough: bool,
    /// GFM bare-URL/email autolink extension. On by default to match the
    /// product's GFM profile. Set this to `false` for strict CommonMark,
    /// where bare URLs stay literal.
    #[serde(default = "default_true")]
    pub gfm_autolink: bool,
    /// micromark construct names to disable, e.g. `"codeIndented"`, `"autolink"`.
    #[serde(default)]
    pub disable: Vec<String>,
    /// When false, raw HTML is emitted as `Text` (escaped by renderers) instead of
    /// a verbatim `Raw { format: "html" }` node — mirroring micromark with
    /// `allowDangerousHtml: false`. No effect on the parser's product default (true).
    #[serde(default = "default_dangerous_html")]
    pub allow_dangerous_html: bool,
    /// Reserved for the conformance harness. The parser currently sanitizes
    /// non-`http(s)`/relative URLs unconditionally (matching micromark's safe
    /// default); setting this to `false` is accepted but has no additional effect.
    /// Wiring `allowDangerousProtocol: true` (pass dangerous protocols through) is
    /// deferred until a conformance case requires it.
    #[serde(default = "default_dangerous_protocol")]
    pub allow_dangerous_protocol: bool,
    /// micromark deviation: a link reference definition indented 4+ spaces (or a
    /// tab) is still recognised when it immediately follows another link
    /// reference definition (no blank line between). CommonMark §5.2 makes any
    /// 4-space-indented line an indented code block; micromark relaxes this for
    /// "subsequent" definitions. Off by default (strict CommonMark); the
    /// conformance harness enables it for the micromark source only.
    #[serde(default)]
    pub subsequent_indented_definitions: bool,
}

fn default_true() -> bool {
    true
}
fn default_dangerous_html() -> bool {
    true
}
fn default_dangerous_protocol() -> bool {
    true
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            gfm_tables: true,
            gfm_strikethrough: true,
            gfm_autolink: true,
            disable: Vec::new(),
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            subsequent_indented_definitions: false,
        }
    }
}

pub fn parse(source: &str) -> SupramarkNode {
    parse_with_options(source, ParseOptions::default())
}

pub fn parse_with_options(source: &str, options: ParseOptions) -> SupramarkNode {
    let allow_dangerous_html = options.allow_dangerous_html;
    let md = create_parser(options);
    let index = OffsetIndex::new(source);
    let (mut children, diagnostics) = map_document(source, &md, &index);
    assign_footnote_indices(&mut children);
    if !allow_dangerous_html {
        for child in &mut children {
            sanitize_raw_html(child);
        }
    }
    SupramarkNode::Root {
        ast_version: 2,
        children,
        diagnostics,
        parser: Some(ParserInfo {
            name: "supramark-markdown".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
        position: Some(root_position(source, &index)),
    }
}

/// When `allow_dangerous_html` is off, raw HTML must reach the output as escaped
/// text (mirroring micromark's safe-by-default HTML serialiser). The parser still
/// recognises HTML blocks/inline so their source span is preserved; this pass
/// rewrites each `Raw { format: "html" }` node into a `Text` node holding the
/// verbatim source, which downstream renderers escape. Both the ast-projection
/// target (`astToHtml`) and the production web renderer escape `text`, so this
/// single pass fixes both comparison targets.
fn sanitize_raw_html(node: &mut SupramarkNode) {
    if let Some(children) = children_mut(node) {
        for child in children {
            sanitize_raw_html(child);
        }
    }
    let owned = std::mem::replace(
        node,
        SupramarkNode::Text {
            value: String::new(),
            position: None,
        },
    );
    *node = match owned {
        SupramarkNode::Raw {
            format,
            value,
            position,
            ..
        } if format == "html" => SupramarkNode::Text { value, position },
        other => other,
    };
}

/// Mutable access to a node's child list, if it has one. Lets the AST walkers
/// recurse without enumerating every variant inline.
fn children_mut(node: &mut SupramarkNode) -> Option<&mut Vec<SupramarkNode>> {
    match node {
        SupramarkNode::Root { children, .. }
        | SupramarkNode::Paragraph { children, .. }
        | SupramarkNode::Heading { children, .. }
        | SupramarkNode::Strong { children, .. }
        | SupramarkNode::Emphasis { children, .. }
        | SupramarkNode::Link { children, .. }
        | SupramarkNode::Delete { children, .. }
        | SupramarkNode::List { children, .. }
        | SupramarkNode::ListItem { children, .. }
        | SupramarkNode::Blockquote { children, .. }
        | SupramarkNode::Table { children, .. }
        | SupramarkNode::TableRow { children, .. }
        | SupramarkNode::TableCell { children, .. }
        | SupramarkNode::DefinitionList { children, .. }
        | SupramarkNode::DefinitionItem { children, .. }
        | SupramarkNode::DefinitionTerm { children, .. }
        | SupramarkNode::DefinitionDescription { children, .. }
        | SupramarkNode::FootnoteDefinition { children, .. }
        | SupramarkNode::Container { children, .. }
        | SupramarkNode::Input { children, .. }
        | SupramarkNode::Unsupported { children, .. } => Some(children),
        _ => None,
    }
}

fn create_parser(options: ParseOptions) -> MarkdownParser {
    let mut md = MarkdownParser::new();
    crate::plugins::cmark::add(&mut md);

    #[cfg(feature = "raw-html")]
    crate::plugins::html::add(&mut md);

    #[cfg(feature = "math")]
    crate::plugins::extra::math::add(&mut md);

    #[cfg(feature = "footnote")]
    crate::plugins::extra::footnote::add(&mut md);

    #[cfg(any(feature = "container", feature = "input"))]
    crate::plugins::extra::ext::add(&mut md);

    #[cfg(feature = "definition-list")]
    crate::plugins::extra::deflist::add(&mut md);

    if options.gfm_tables {
        crate::plugins::extra::tables::add(&mut md);
    }
    if options.gfm_strikethrough {
        crate::plugins::extra::strikethrough::add(&mut md);
    }
    if options.gfm_autolink {
        // GFM autolink extension: bare www./scheme-URL/email linkification.
        crate::plugins::extra::gfm_autolink::add(&mut md);
    }
    md.subsequent_indented_definitions = options.subsequent_indented_definitions;

    apply_disabled(&mut md, &options.disable);

    md
}

/// Remove rules for constructs named in `disabled` (micromark construct names).
/// Unknown names are ignored so future micromark constructs don't break us.
fn apply_disabled(md: &mut MarkdownParser, disabled: &[String]) {
    use crate::plugins::cmark::block::{
        blockquote::BlockquoteScanner, code::CodeScanner, fence::FenceScanner,
        heading::HeadingScanner, hr::HrScanner, lheading::LHeadingScanner, list::ListScanner,
        reference::ReferenceScanner,
    };
    use crate::plugins::cmark::inline::{autolink::AutolinkScanner};
    #[cfg(feature = "raw-html")]
    use crate::plugins::html::{html_block::HtmlBlockScanner, html_inline::HtmlInlineScanner};
    use crate::generics::inline::{
        code_pair::CodePairScanner, emph_pair::EmphPairScanner,
        full_link::{LinkPrefixScanner, LinkScanner},
    };

    for name in disabled {
        match name.as_str() {
            // --- block constructs ---
            "codeIndented" => {
                md.block.remove_rule::<CodeScanner>();
                // `code::add` sets max_indent=4 as a side effect. With indented
                // code disabled, restore the unconstrained indent so other block
                // rules don't keep gating on 4-space indent.
                md.max_indent = i32::MAX;
            }
            "codeFenced" => md.block.remove_rule::<FenceScanner>(),
            "headingAtx" => md.block.remove_rule::<HeadingScanner>(),
            "setextUnderline" => md.block.remove_rule::<LHeadingScanner>(),
            "thematicBreak" => md.block.remove_rule::<HrScanner>(),
            "blockQuote" => md.block.remove_rule::<BlockquoteScanner>(),
            "list" => md.block.remove_rule::<ListScanner>(),
            #[cfg(feature = "raw-html")]
            "htmlFlow" => md.block.remove_rule::<HtmlBlockScanner>(),
            "definition" => md.block.remove_rule::<ReferenceScanner>(),
            // --- inline constructs ---
            "autolink" => md.inline.remove_rule::<AutolinkScanner>(),
            // `characterEscape`/`hardBreakEscape` share `EscapeScanner`; gate
            // each branch at runtime so the two constructs disable independently.
            "characterEscape" => md.disable_character_escape = true,
            "codeText" => md.inline.remove_rule::<CodePairScanner<'`'>>(),
            "attention" => {
                md.inline.remove_rule::<EmphPairScanner<'*', true>>();
                md.inline.remove_rule::<EmphPairScanner<'_', false>>();
            }
            "hardBreakEscape" => md.disable_hard_break_escape = true,
            #[cfg(feature = "raw-html")]
            "htmlText" => md.inline.remove_rule::<HtmlInlineScanner>(),
            "labelStartLink" => md.inline.remove_rule::<LinkScanner<false>>(),
            "labelStartImage" => md.inline.remove_rule::<LinkPrefixScanner<'!', true>>(),
            // `labelEnd` can't be disabled by removing `LinkScannerEnd` —
            // `parse_link_label` finds `]` by raw char match, so the sentinel's
            // removal is a no-op for resolution. Gate the openers at runtime
            // instead; with resolution refused, `[x]()` stays literal.
            "labelEnd" => md.disable_label_end = true,
            _ => {}
        }
    }
}

fn map_markdown_fragment(
    md: &MarkdownParser,
    source: &str,
    start: usize,
    end: usize,
    index: &OffsetIndex,
) -> Vec<SupramarkNode> {
    if source[start..end].trim().is_empty() {
        return Vec::new();
    }

    let root = md.parse(&source[start..end]);
    map_children(&root.children, index, start)
}

/// Context threaded through in-rule AST v2 construction.
///
/// Holds the immutable offset index and document base offset so a node's
/// `to_ast_v2` impl can compute positions and recurse into children without
/// re-plumbing those arguments by hand.
pub(crate) struct AstV2Ctx<'a> {
    index: &'a OffsetIndex,
    base_offset: usize,
}

impl<'a> AstV2Ctx<'a> {
    pub(crate) fn position(&self, node: &Node) -> Option<SourcePosition> {
        position_for(node, self.index, self.base_offset)
    }

    pub(crate) fn map_children(&self, children: &[Node]) -> Vec<SupramarkNode> {
        map_children(children, self.index, self.base_offset)
    }

    pub(crate) fn map_inline_text(&self, value: &str, node: &Node) -> Vec<SupramarkNode> {
        map_inline_text(value, self.position(node), self.index)
    }

    pub(crate) fn map_fence(&self, fence: &CodeFence, node: &Node) -> SupramarkNode {
        map_fence(fence, self.position(node))
    }

    pub(crate) fn map_list_item_children(
        &self,
        children: &[Node],
    ) -> (Option<bool>, Vec<SupramarkNode>) {
        map_list_item_children(children, self.index, self.base_offset)
    }

    pub(crate) fn map_table_sections(
        &self,
        sections: &[Node],
        alignments: &[ColumnAlignment],
    ) -> Vec<SupramarkNode> {
        map_table_sections(sections, alignments, self.index, self.base_offset)
    }
}

fn map_children(children: &[Node], index: &OffsetIndex, base_offset: usize) -> Vec<SupramarkNode> {
    let mapped = children
        .iter()
        .flat_map(|child| map_node(child, index, base_offset))
        .collect();
    let mapped = reassemble_split_footnote_refs(mapped, index);
    rescan_adjacent_text_runs(mapped, index)
}

/// Normalize a footnote label into an mdast-style identifier used to match
/// references with definitions: trim leading/trailing whitespace, collapse
/// internal whitespace runs into a single space, and lowercase.
pub(crate) fn normalize_footnote_identifier(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn assign_footnote_indices(children: &mut [SupramarkNode]) {
    let mut labels = HashMap::new();
    let mut next_index = 1;

    collect_footnote_reference_labels(children, &mut labels, &mut next_index);
    collect_footnote_definition_labels(children, &mut labels, &mut next_index);
    apply_footnote_indices(children, &labels);
}

fn collect_footnote_reference_labels(
    nodes: &[SupramarkNode],
    labels: &mut HashMap<String, u32>,
    next_index: &mut u32,
) {
    for node in nodes {
        match node {
            SupramarkNode::FootnoteReference { identifier, .. } => {
                assign_footnote_label(identifier, labels, next_index);
            }
            _ => visit_children(node, |children| {
                collect_footnote_reference_labels(children, labels, next_index);
            }),
        }
    }
}

fn collect_footnote_definition_labels(
    nodes: &[SupramarkNode],
    labels: &mut HashMap<String, u32>,
    next_index: &mut u32,
) {
    for node in nodes {
        match node {
            SupramarkNode::FootnoteDefinition {
                identifier,
                children,
                ..
            } => {
                assign_footnote_label(identifier, labels, next_index);
                collect_footnote_definition_labels(children, labels, next_index);
            }
            _ => visit_children(node, |children| {
                collect_footnote_definition_labels(children, labels, next_index);
            }),
        }
    }
}

fn assign_footnote_label(label: &str, labels: &mut HashMap<String, u32>, next_index: &mut u32) {
    if labels.contains_key(label) {
        return;
    }
    let index = *next_index;
    labels.insert(label.to_owned(), index);
    *next_index += 1;
}

fn apply_footnote_indices(nodes: &mut [SupramarkNode], labels: &HashMap<String, u32>) {
    for node in nodes {
        match node {
            SupramarkNode::FootnoteReference {
                index, identifier, ..
            }
            | SupramarkNode::FootnoteDefinition {
                index, identifier, ..
            } => {
                *index = labels.get(identifier).copied().unwrap_or(0);
                visit_children_mut(node, |children| apply_footnote_indices(children, labels));
            }
            _ => visit_children_mut(node, |children| apply_footnote_indices(children, labels)),
        }
    }
}

fn visit_children<F>(node: &SupramarkNode, mut visit: F)
where
    F: FnMut(&[SupramarkNode]),
{
    match node {
        SupramarkNode::Root { children, .. }
        | SupramarkNode::Paragraph { children, .. }
        | SupramarkNode::Heading { children, .. }
        | SupramarkNode::Strong { children, .. }
        | SupramarkNode::Emphasis { children, .. }
        | SupramarkNode::Delete { children, .. }
        | SupramarkNode::List { children, .. }
        | SupramarkNode::ListItem { children, .. }
        | SupramarkNode::Blockquote { children, .. }
        | SupramarkNode::Table { children, .. }
        | SupramarkNode::TableRow { children, .. }
        | SupramarkNode::TableCell { children, .. }
        | SupramarkNode::DefinitionList { children, .. }
        | SupramarkNode::DefinitionItem { children, .. }
        | SupramarkNode::DefinitionTerm { children, .. }
        | SupramarkNode::DefinitionDescription { children, .. }
        | SupramarkNode::FootnoteDefinition { children, .. }
        | SupramarkNode::Container { children, .. }
        | SupramarkNode::Input { children, .. }
        | SupramarkNode::Unsupported { children, .. } => visit(children),
        _ => {}
    }
}

fn visit_children_mut<F>(node: &mut SupramarkNode, mut visit: F)
where
    F: FnMut(&mut [SupramarkNode]),
{
    match node {
        SupramarkNode::Root { children, .. }
        | SupramarkNode::Paragraph { children, .. }
        | SupramarkNode::Heading { children, .. }
        | SupramarkNode::Strong { children, .. }
        | SupramarkNode::Emphasis { children, .. }
        | SupramarkNode::Delete { children, .. }
        | SupramarkNode::List { children, .. }
        | SupramarkNode::ListItem { children, .. }
        | SupramarkNode::Blockquote { children, .. }
        | SupramarkNode::Table { children, .. }
        | SupramarkNode::TableRow { children, .. }
        | SupramarkNode::TableCell { children, .. }
        | SupramarkNode::DefinitionList { children, .. }
        | SupramarkNode::DefinitionItem { children, .. }
        | SupramarkNode::DefinitionTerm { children, .. }
        | SupramarkNode::DefinitionDescription { children, .. }
        | SupramarkNode::FootnoteDefinition { children, .. }
        | SupramarkNode::Container { children, .. }
        | SupramarkNode::Input { children, .. }
        | SupramarkNode::Unsupported { children, .. } => visit(children),
        _ => {}
    }
}

fn map_node(node: &Node, index: &OffsetIndex, base_offset: usize) -> Vec<SupramarkNode> {
    let ctx = AstV2Ctx { index, base_offset };
    if let Some(v2) = node.to_ast_v2(&ctx) {
        return v2;
    }

    map_children(&node.children, index, base_offset)
}

fn map_list_item_children(
    children: &[Node],
    index: &OffsetIndex,
    base_offset: usize,
) -> (Option<bool>, Vec<SupramarkNode>) {
    let mut mapped = map_children(children, index, base_offset);
    let checked = strip_task_marker(&mut mapped);
    (checked, mapped)
}

fn map_inline_text(
    value: &str,
    position: Option<SourcePosition>,
    index: &OffsetIndex,
) -> Vec<SupramarkNode> {
    let Some(position) = position else {
        return vec![SupramarkNode::Text {
            value: replace_emoji_shortcodes(value),
            position: None,
        }];
    };

    let source_start = position.start.byte_offset;
    if position.end.byte_offset.saturating_sub(source_start) != value.len() {
        return vec![SupramarkNode::Text {
            value: replace_emoji_shortcodes(value),
            position: Some(position),
        }];
    }

    let mut nodes = Vec::new();
    let mut cursor = 0;

    while cursor < value.len() {
        let Some(next) = find_next_inline_extension(value, cursor) else {
            push_text_slice(&mut nodes, value, cursor, value.len(), source_start, index);
            break;
        };

        push_text_slice(&mut nodes, value, cursor, next.start, source_start, index);

        match next.kind {
            InlineExtensionKind::Math { content_start, end } => {
                nodes.push(SupramarkNode::MathInline {
                    value: value[content_start..end].to_owned(),
                    position: Some(position_from_abs(
                        index,
                        source_start + next.start,
                        source_start + end + 1,
                    )),
                });
                cursor = end + 1;
            }
            InlineExtensionKind::Footnote { label_start, end } => {
                let label = value[label_start..end].to_owned();
                nodes.push(SupramarkNode::FootnoteReference {
                    index: 0,
                    identifier: normalize_footnote_identifier(&label),
                    label,
                    position: Some(position_from_abs(
                        index,
                        source_start + next.start,
                        source_start + end + 1,
                    )),
                });
                cursor = end + 1;
            }
        }
    }

    if nodes.is_empty() {
        nodes.push(SupramarkNode::Text {
            value: replace_emoji_shortcodes(value),
            position: Some(position),
        });
    }

    nodes
}

/// Reassemble a footnote reference `[^label]` whose label was split by an
/// inline raw-HTML token (e.g. `[^"><script>…</script>]`). cmark-gfm treats
/// the entire `[…]` as the label string — it does not render the embedded
/// HTML — so the reference is reconstructed by absorbing the intervening
/// `Raw(html)`/`Text` siblings up to the closing `]`. Without this, the
/// raw-HTML inline rule consumes the `<…>` before the footnote post-process
/// sees the full `[^…]`, leaking the tag into the output (an XSS vector).
fn reassemble_split_footnote_refs(
    nodes: Vec<SupramarkNode>,
    index: &OffsetIndex,
) -> Vec<SupramarkNode> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut i = 0;
    while i < nodes.len() {
        let (text_before, after_caret, open_pos, open_value_len, open_k) = match &nodes[i] {
            SupramarkNode::Text { value, position } => {
                match find_unclosed_footnote_open(value) {
                    Some((tb, ac)) => {
                        let k = value.len() - ac.len() - 2; // index of '['
                        (tb, ac, position.clone(), value.len(), k)
                    }
                    None => {
                        out.push(nodes[i].clone());
                        i += 1;
                        continue;
                    }
                }
            }
            _ => {
                out.push(nodes[i].clone());
                i += 1;
                continue;
            }
        };
        let mut label = after_caret.to_owned();
        let mut found_close = None;
        let mut j = i + 1;
        while j < nodes.len() {
            match &nodes[j] {
                SupramarkNode::Raw { format, value, .. } if format == "html" => {
                    label.push_str(value);
                    j += 1;
                }
                SupramarkNode::Text { value, position } => match value.find(']') {
                    Some(br) => {
                        label.push_str(&value[..br]);
                        found_close = Some(Close {
                            idx: j,
                            br,
                            value_len: value.len(),
                            text_after: value[br + 1..].to_owned(),
                            position: position.clone(),
                        });
                        break;
                    }
                    None => {
                        label.push_str(value);
                        j += 1;
                    }
                },
                _ => break,
            }
        }
        match found_close {
            Some(close) => {
                if !text_before.is_empty() {
                    out.push(SupramarkNode::Text {
                        value: text_before.to_owned(),
                        position: sub_text_position(
                            index,
                            open_pos.as_ref(),
                            open_value_len,
                            0,
                            open_k,
                        ),
                    });
                }
                out.push(SupramarkNode::FootnoteReference {
                    index: 0,
                    identifier: normalize_footnote_identifier(&label),
                    label,
                    position: ref_position(
                        index,
                        open_pos.as_ref(),
                        open_value_len,
                        open_k,
                        close.position.as_ref(),
                        close.value_len,
                        close.br,
                    ),
                });
                if !close.text_after.is_empty() {
                    out.push(SupramarkNode::Text {
                        value: close.text_after,
                        position: sub_text_position(
                            index,
                            close.position.as_ref(),
                            close.value_len,
                            close.br + 1,
                            close.value_len,
                        ),
                    });
                }
                i = close.idx + 1;
            }
            None => {
                out.push(nodes[i].clone());
                i += 1;
            }
        }
    }
    out
}

struct Close {
    idx: usize,
    br: usize,
    value_len: usize,
    text_after: String,
    position: Option<SourcePosition>,
}

/// Slice a parent Text node's position to cover `value[lo..hi]`. Only valid
/// when the value's byte length equals the position span (the invariant
/// `map_inline_text` relies on); otherwise `None` is safer than a wrong
/// sub-position.
fn sub_text_position(
    index: &OffsetIndex,
    parent: Option<&SourcePosition>,
    value_len: usize,
    lo: usize,
    hi: usize,
) -> Option<SourcePosition> {
    let parent = parent?;
    if parent.end.byte_offset.saturating_sub(parent.start.byte_offset) != value_len {
        return None;
    }
    Some(SourcePosition {
        start: index.point_at(parent.start.byte_offset + lo),
        end: index.point_at(parent.start.byte_offset + hi),
    })
}

/// Position for a reassembled `[^…]` footnote reference: from the `[` in the
/// opening text node to just past the closing `]` in the closing text node.
fn ref_position(
    index: &OffsetIndex,
    open: Option<&SourcePosition>,
    open_value_len: usize,
    open_k: usize,
    close: Option<&SourcePosition>,
    close_value_len: usize,
    close_br: usize,
) -> Option<SourcePosition> {
    let open = open?;
    let close = close?;
    if open.end.byte_offset.saturating_sub(open.start.byte_offset) != open_value_len {
        return None;
    }
    if close.end.byte_offset.saturating_sub(close.start.byte_offset) != close_value_len {
        return None;
    }
    Some(SourcePosition {
        start: index.point_at(open.start.byte_offset + open_k),
        end: index.point_at(close.start.byte_offset + close_br + 1),
    })
}

/// If `value` contains a `[^` whose remainder has no closing `]` in the same
/// text node, return `(text_before_caret, label_so_far)`. Otherwise `None`
/// (the reference is either absent or complete within this node, so the
/// normal per-text scan handles it).
fn find_unclosed_footnote_open(value: &str) -> Option<(&str, &str)> {
    let bytes = value.as_bytes();
    let mut k = 0;
    while k + 1 < bytes.len() {
        if bytes[k] == b'[' && bytes[k + 1] == b'^' {
            let rest = &value[k + 2..];
            if !rest.contains(']') {
                return Some((&value[..k], rest));
            }
        }
        k += 1;
    }
    None
}

fn rescan_adjacent_text_runs(nodes: Vec<SupramarkNode>, index: &OffsetIndex) -> Vec<SupramarkNode> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut run_value = String::new();
    let mut run_position: Option<SourcePosition> = None;

    for node in nodes {
        match node {
            SupramarkNode::Text { value, position } => {
                if let Some(existing) = run_position.as_mut() {
                    if position.as_ref().is_some_and(|position| {
                        existing.end.byte_offset == position.start.byte_offset
                    }) {
                        run_value.push_str(&value);
                        existing.end = position.expect("checked by is_some_and").end;
                        continue;
                    }
                }
                flush_text_run(&mut out, &mut run_value, &mut run_position, index);
                run_value = value;
                run_position = position;
            }
            node => {
                flush_text_run(&mut out, &mut run_value, &mut run_position, index);
                out.push(node);
            }
        }
    }

    flush_text_run(&mut out, &mut run_value, &mut run_position, index);
    out
}

fn flush_text_run(
    out: &mut Vec<SupramarkNode>,
    value: &mut String,
    position: &mut Option<SourcePosition>,
    index: &OffsetIndex,
) {
    if value.is_empty() {
        *position = None;
        return;
    }

    let value_out = std::mem::take(value);
    let position_out = position.take();
    if inline_extension_scan_needed(&value_out) {
        out.extend(map_inline_text(&value_out, position_out, index));
    } else {
        out.push(SupramarkNode::Text {
            value: value_out,
            position: position_out,
        });
    }
}

fn inline_extension_scan_needed(value: &str) -> bool {
    value.contains('$') || value.contains("[^")
}

#[derive(Debug, Clone, Copy)]
struct InlineExtension {
    start: usize,
    kind: InlineExtensionKind,
}

#[derive(Debug, Clone, Copy)]
enum InlineExtensionKind {
    Math { content_start: usize, end: usize },
    Footnote { label_start: usize, end: usize },
}

fn find_next_inline_extension(value: &str, from: usize) -> Option<InlineExtension> {
    let mut cursor = from;

    while cursor < value.len() {
        let mut chars = value[cursor..].char_indices();
        let (relative, ch) = chars.next()?;
        let index = cursor + relative;

        if ch == '$' && !is_escaped(value, index) {
            if let Some(end) = find_closing_math_delimiter(value, index + 1) {
                if end > index + 1 {
                    return Some(InlineExtension {
                        start: index,
                        kind: InlineExtensionKind::Math {
                            content_start: index + 1,
                            end,
                        },
                    });
                }
            }
        }

        if ch == '['
            && !is_escaped(value, index)
            && value[index..].starts_with("[^")
            && index + 2 < value.len()
        {
            if let Some(close_relative) = value[index + 2..].find(']') {
                let end = index + 2 + close_relative;
                if end > index + 2 {
                    return Some(InlineExtension {
                        start: index,
                        kind: InlineExtensionKind::Footnote {
                            label_start: index + 2,
                            end,
                        },
                    });
                }
            }
        }

        cursor = index + ch.len_utf8();
    }

    None
}

fn find_closing_math_delimiter(value: &str, from: usize) -> Option<usize> {
    let mut cursor = from;
    while cursor < value.len() {
        let relative = value[cursor..].find('$')?;
        let index = cursor + relative;
        if !is_escaped(value, index) && !value[from..index].contains('\n') {
            return Some(index);
        }
        cursor = index + 1;
    }
    None
}

fn is_escaped(value: &str, byte_index: usize) -> bool {
    let mut count = 0;
    for byte in value[..byte_index].bytes().rev() {
        if byte == b'\\' {
            count += 1;
        } else {
            break;
        }
    }
    count % 2 == 1
}

fn push_text_slice(
    nodes: &mut Vec<SupramarkNode>,
    value: &str,
    start: usize,
    end: usize,
    source_start: usize,
    index: &OffsetIndex,
) {
    if start >= end {
        return;
    }

    nodes.push(SupramarkNode::Text {
        value: replace_emoji_shortcodes(&value[start..end]),
        position: Some(position_from_abs(
            index,
            source_start + start,
            source_start + end,
        )),
    });
}

fn replace_emoji_shortcodes(value: &str) -> String {
    // CommonMark §2.3: U+0000 in the source becomes U+FFFD. Done here on the
    // AST text value (not the source) so source-map byte offsets stay aligned
    // with the original input — "\0".len() != "\u{FFFD}".len().
    let value = value.replace('\0', "\u{FFFD}");
    if !value.contains(':') {
        return value;
    }

    let mut output = String::with_capacity(value.len());
    let mut cursor = 0;

    while let Some(relative_start) = value[cursor..].find(':') {
        let start = cursor + relative_start;
        output.push_str(&value[cursor..start]);

        if let Some(relative_end) = value[start + 1..].find(':') {
            let end = start + 1 + relative_end;
            let name = &value[start + 1..end];
            if is_emoji_shortcode_name(name) {
                if let Some(emoji) = emoji_shortcode(name) {
                    output.push_str(emoji);
                    cursor = end + 1;
                    continue;
                }
            }
        }

        output.push(':');
        cursor = start + 1;
    }

    output.push_str(&value[cursor..]);
    output
}

fn is_emoji_shortcode_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn emoji_shortcode(name: &str) -> Option<&'static str> {
    match name {
        "smile" => Some("😄"),
        "joy" => Some("😂"),
        "wink" => Some("😉"),
        "rocket" => Some("🚀"),
        "tada" => Some("🎉"),
        "warning" => Some("⚠️"),
        "heart" => Some("❤️"),
        "coffee" => Some("☕"),
        "tea" => Some("🍵"),
        _ => None,
    }
}

fn strip_task_marker(nodes: &mut [SupramarkNode]) -> Option<bool> {
    let first = nodes.first_mut()?;
    match first {
        SupramarkNode::Paragraph { children, .. } => strip_task_marker(children),
        SupramarkNode::Text { value, .. } => strip_task_marker_from_text(value),
        _ => None,
    }
}

fn strip_task_marker_from_text(value: &mut String) -> Option<bool> {
    let trimmed = value.trim_start();
    let leading_len = value.len() - trimmed.len();

    let (checked, marker_len) = if trimmed.starts_with("[ ]") {
        (false, 3)
    } else if trimmed
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("[x]"))
    {
        (true, 3)
    } else {
        return None;
    };

    let rest = &trimmed[marker_len..];
    // cmark-gfm's tasklist scan (ext_scanners.re) requires one or more
    // spacechars (`[ \t]`) after the `[ ]`/`[x]` marker and consumes them as
    // the separator — the text content starts after the separator, and the
    // single space in the HTML output comes from the renderer's literal
    // (`"<input ... /> "`), not from the text node. Mirror that:
    //   * no separator whitespace => not a task-list item (cmark leaves
    //     `[ ]foo` literal), so bail;
    //   * a separator => consume all of it so the text node is `foo`, not
    //     `\tfoo` / `  foo`.
    // Previously the raw separator was preserved, which forced the web
    // renderer to rely on whitespace-collapse to match cmark and left RN
    // showing a stray tab / doubled space.
    let content = rest.trim_start_matches(|c| c == ' ' || c == '\t');
    if content.len() == rest.len() {
        return None;
    }
    let mut replacement = String::with_capacity(leading_len + content.len());
    replacement.push_str(&value[..leading_len]);
    replacement.push_str(content);
    *value = replacement;
    Some(checked)
}

fn map_fence(fence: &CodeFence, position: Option<SourcePosition>) -> SupramarkNode {
    let info = crate::common::utils::unescape_all(fence.info.trim());
    let mut parts = info.split_whitespace();
    let lang = parts.next().map(str::to_owned);
    let meta_raw = {
        let rest = parts.collect::<Vec<_>>().join(" ");
        (!rest.is_empty()).then_some(rest)
    };

    if let Some(engine) = lang.as_deref().and_then(diagram_engine) {
        SupramarkNode::Diagram {
            engine: engine.to_owned(),
            code: fence.content.clone(),
            fence_closed: fence.closed,
            meta: meta_raw.as_deref().and_then(parse_diagram_meta),
            semantic: None,
            position,
        }
    } else {
        SupramarkNode::Code {
            value: fence.content.clone(),
            lang,
            meta: meta_raw,
            position,
        }
    }
}

/// Parse the fence info string remainder (everything after the language token)
/// into a structured JSON object.
///
/// Syntax: whitespace-separated items. Each item is split on its first `=` into
/// key/value; a double-quote wrapped value has the quotes stripped. A bare item
/// without `=` becomes `key = true`. An empty/whitespace-only remainder yields
/// `None` so the field is omitted rather than serialized as an empty object.
fn parse_diagram_meta(meta: &str) -> Option<serde_json::Value> {
    let mut object = serde_json::Map::new();
    for item in meta.split_whitespace() {
        if let Some((key, value)) = item.split_once('=') {
            if key.is_empty() {
                continue;
            }
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            object.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
        } else {
            object.insert(item.to_owned(), serde_json::Value::Bool(true));
        }
    }
    if object.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(object))
    }
}

fn diagram_engine(lang: &str) -> Option<&str> {
    match lang.to_ascii_lowercase().as_str() {
        "mermaid" => Some("mermaid"),
        "plantuml" => Some("plantuml"),
        "vega" => Some("vega"),
        "vega-lite" => Some("vega-lite"),
        "echarts" => Some("echarts"),
        "chart" => Some("chart"),
        "chartjs" => Some("chartjs"),
        "chart.js" => Some("chart.js"),
        "plotly" => Some("plotly"),
        "dot" => Some("dot"),
        "graphviz" => Some("graphviz"),
        "d2" => Some("d2"),
        _ => None,
    }
}

fn map_table_sections(
    sections: &[Node],
    alignments: &[ColumnAlignment],
    index: &OffsetIndex,
    base_offset: usize,
) -> Vec<SupramarkNode> {
    let mut rows = Vec::new();
    for section in sections {
        let header = section.is::<TableHead>();
        if header || section.is::<TableBody>() {
            rows.extend(map_table_rows(
                &section.children,
                alignments,
                header,
                index,
                base_offset,
            ));
        } else {
            rows.extend(map_node(section, index, base_offset));
        }
    }
    rows
}

fn map_table_rows(
    rows: &[Node],
    alignments: &[ColumnAlignment],
    header: bool,
    index: &OffsetIndex,
    base_offset: usize,
) -> Vec<SupramarkNode> {
    rows.iter()
        .flat_map(|row| {
            if row.is::<TableRow>() {
                vec![SupramarkNode::TableRow {
                    children: map_table_cells(
                        &row.children,
                        alignments,
                        header,
                        index,
                        base_offset,
                    ),
                    position: position_for(row, index, base_offset),
                }]
            } else {
                map_node(row, index, base_offset)
            }
        })
        .collect()
}

fn map_table_cells(
    cells: &[Node],
    alignments: &[ColumnAlignment],
    header: bool,
    index: &OffsetIndex,
    base_offset: usize,
) -> Vec<SupramarkNode> {
    cells
        .iter()
        .enumerate()
        .flat_map(|(column, cell)| {
            if cell.is::<TableCell>() {
                vec![SupramarkNode::TableCell {
                    align: alignments.get(column).and_then(map_alignment),
                    header,
                    children: map_children(&cell.children, index, base_offset),
                    position: position_for(cell, index, base_offset),
                }]
            } else {
                map_node(cell, index, base_offset)
            }
        })
        .collect()
}

pub(crate) fn map_alignment(alignment: &ColumnAlignment) -> Option<TableAlign> {
    match alignment {
        ColumnAlignment::None => None,
        ColumnAlignment::Left => Some(TableAlign::Left),
        ColumnAlignment::Right => Some(TableAlign::Right),
        ColumnAlignment::Center => Some(TableAlign::Center),
    }
}

fn position_for(node: &Node, index: &OffsetIndex, base_offset: usize) -> Option<SourcePosition> {
    let (start, end) = node.srcmap?.get_byte_offsets();
    Some(position_from_abs(
        index,
        base_offset + start,
        base_offset + end,
    ))
}

fn position_from_abs(index: &OffsetIndex, start: usize, end: usize) -> SourcePosition {
    SourcePosition {
        start: index.point_at(start),
        end: index.point_at(end),
    }
}

fn root_position(source: &str, index: &OffsetIndex) -> SourcePosition {
    position_from_abs(index, 0, source.len())
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ExtensionSyntax {
    Container,
    Input,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtensionOpen {
    pub(crate) syntax: ExtensionSyntax,
    pub(crate) name: String,
    pub(crate) params: Option<String>,
    pub(crate) close_marker: &'static str,
}

impl ExtensionOpen {
    fn syntax_name(&self) -> &'static str {
        match self.syntax {
            ExtensionSyntax::Container => "container",
            ExtensionSyntax::Input => "input",
        }
    }
}

pub(crate) fn parse_extension_open(line: &str) -> Option<ExtensionOpen> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix(":::") {
        return parse_named_extension(rest, ExtensionSyntax::Container, ":::");
    }
    if let Some(rest) = trimmed.strip_prefix("%%%") {
        return parse_named_extension(rest, ExtensionSyntax::Input, "%%%");
    }
    None
}

fn parse_named_extension(
    rest: &str,
    syntax: ExtensionSyntax,
    close_marker: &'static str,
) -> Option<ExtensionOpen> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next()?.trim().to_ascii_lowercase();
    if !is_valid_extension_name(&name) {
        return None;
    }
    let params = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    Some(ExtensionOpen {
        syntax,
        name,
        params,
        close_marker,
    })
}

fn is_valid_extension_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn parse_vison_data(value: &str) -> serde_json::Value {
    let trimmed = value.trim();
    let mut object = serde_json::Map::new();
    object.insert(
        "source".to_owned(),
        serde_json::Value::String(value.to_owned()),
    );

    if trimmed.is_empty() {
        object.insert(
            "parseError".to_owned(),
            serde_json::Value::String("empty body".to_owned()),
        );
        return serde_json::Value::Object(object);
    }

    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(spec @ serde_json::Value::Object(_)) => {
            object.insert("spec".to_owned(), spec);
        }
        Ok(_) => {
            object.insert(
                "parseError".to_owned(),
                serde_json::Value::String("parsed value is not a JSON object".to_owned()),
            );
        }
        Err(error) => {
            object.insert(
                "parseError".to_owned(),
                serde_json::Value::String(error.to_string()),
            );
        }
    }

    serde_json::Value::Object(object)
}

fn parse_weather_data(params: Option<&str>, value: &str) -> serde_json::Value {
    let format = parse_weather_format(params);
    let mut object = serde_json::Map::new();
    object.insert(
        "format".to_owned(),
        serde_json::Value::String(format.to_owned()),
    );

    let parsed = match format {
        "json" => parse_weather_json_config(value),
        "toon" => Ok(parse_weather_key_value_config(value)),
        _ => Ok(parse_weather_key_value_config(value)),
    };

    match parsed {
        Ok(config) => {
            copy_weather_field(&mut object, &config, "location", &["location"]);
            copy_weather_field(&mut object, &config, "units", &["units"]);
            copy_weather_field(
                &mut object,
                &config,
                "showForecast",
                &["showForecast", "show_forecast"],
            );
            copy_weather_field(&mut object, &config, "days", &["days"]);
        }
        Err(error) => {
            object.insert("parseError".to_owned(), serde_json::Value::String(error));
            object.insert(
                "rawConfig".to_owned(),
                serde_json::Value::String(value.to_owned()),
            );
        }
    }

    serde_json::Value::Object(object)
}

fn parse_weather_format(params: Option<&str>) -> &'static str {
    match params
        .and_then(|params| params.split_whitespace().next())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => "json",
        Some("toon") => "toon",
        Some("yaml") => "yaml",
        _ => "yaml",
    }
}

fn parse_weather_json_config(
    value: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    match serde_json::from_str::<serde_json::Value>(value.trim()) {
        Ok(serde_json::Value::Object(object)) => Ok(object),
        Ok(_) => Err("weather JSON config must be an object".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_weather_key_value_config(value: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut object = serde_json::Map::new();

    for line in value.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        object.insert(
            key.to_owned(),
            parse_weather_scalar_value(raw_value.trim()).unwrap_or(serde_json::Value::Null),
        );
    }

    object
}

fn parse_weather_scalar_value(raw: &str) -> Option<serde_json::Value> {
    if raw.is_empty() {
        return Some(serde_json::Value::String(String::new()));
    }

    if raw == "true" {
        return Some(serde_json::Value::Bool(true));
    }
    if raw == "false" {
        return Some(serde_json::Value::Bool(false));
    }
    if let Ok(value) = raw.parse::<i64>() {
        return Some(serde_json::Value::Number(value.into()));
    }
    if let Ok(value) = raw.parse::<f64>() {
        if let Some(number) = serde_json::Number::from_f64(value) {
            return Some(serde_json::Value::Number(number));
        }
    }

    let unquoted = if (raw.starts_with('"') && raw.ends_with('"'))
        || (raw.starts_with('\'') && raw.ends_with('\''))
    {
        &raw[1..raw.len().saturating_sub(1)]
    } else {
        raw
    };

    Some(serde_json::Value::String(unquoted.to_owned()))
}

fn copy_weather_field(
    target: &mut serde_json::Map<String, serde_json::Value>,
    source: &serde_json::Map<String, serde_json::Value>,
    output_key: &str,
    input_keys: &[&str],
) {
    if let Some(value) = input_keys.iter().find_map(|key| source.get(*key)) {
        if !value.is_null() {
            target.insert(output_key.to_owned(), value.clone());
        }
    }
}

fn parse_map_data(value: &str) -> Option<serde_json::Value> {
    let mut center: Option<[f64; 2]> = None;
    let mut zoom: Option<f64> = None;
    let mut marker_lat: Option<f64> = None;
    let mut marker_lng: Option<f64> = None;
    let mut in_marker = false;

    for line in value.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|ch| ch.is_whitespace()).count();
        let trimmed = line.trim();

        if indent == 0 {
            in_marker = false;
            if let Some(raw) = trimmed.strip_prefix("center:") {
                center = parse_tuple2(raw.trim());
            } else if let Some(raw) = trimmed.strip_prefix("zoom:") {
                zoom = raw.trim().parse::<f64>().ok();
            } else if trimmed == "marker:" {
                in_marker = true;
            }
        } else if in_marker {
            if let Some(raw) = trimmed.strip_prefix("lat:") {
                marker_lat = raw.trim().parse::<f64>().ok();
            } else if let Some(raw) = trimmed.strip_prefix("lng:") {
                marker_lng = raw.trim().parse::<f64>().ok();
            }
        }
    }

    if center.is_none() && zoom.is_none() && (marker_lat.is_none() || marker_lng.is_none()) {
        return None;
    }

    let mut object = serde_json::Map::new();
    if let Some(center) = center {
        object.insert(
            "center".to_owned(),
            serde_json::json!([center[0], center[1]]),
        );
    }
    if let Some(zoom) = zoom {
        object.insert("zoom".to_owned(), serde_json::json!(zoom));
    }
    if let (Some(lat), Some(lng)) = (marker_lat, marker_lng) {
        object.insert(
            "markers".to_owned(),
            serde_json::json!([{ "lat": lat, "lng": lng }]),
        );
    }

    Some(serde_json::Value::Object(object))
}

fn parse_tuple2(raw: &str) -> Option<[f64; 2]> {
    let raw = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let mut parts = raw.split(',').map(str::trim);
    let first = parts.next()?.parse::<f64>().ok()?;
    let second = parts.next()?.parse::<f64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some([first, second])
}

#[derive(Debug)]
struct OffsetIndex {
    entries: Vec<(usize, SourcePoint)>,
}

impl OffsetIndex {
    fn new(source: &str) -> Self {
        let mut entries = Vec::new();
        let mut line = 1;
        let mut column = 1;
        let mut utf16_offset = 0;

        for (byte_offset, ch) in source.char_indices() {
            entries.push((
                byte_offset,
                SourcePoint {
                    line,
                    column,
                    byte_offset,
                    utf16_offset,
                },
            ));

            utf16_offset += ch.len_utf16();
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }

        let byte_offset = source.len();
        entries.push((
            byte_offset,
            SourcePoint {
                line,
                column,
                byte_offset,
                utf16_offset,
            },
        ));

        Self { entries }
    }

    fn point_at(&self, byte_offset: usize) -> SourcePoint {
        match self
            .entries
            .binary_search_by_key(&byte_offset, |(offset, _)| *offset)
        {
            Ok(index) => self.entries[index].1.clone(),
            Err(0) => self.entries[0].1.clone(),
            Err(index) => self.entries[index - 1].1.clone(),
        }
    }
}

fn map_document(
    source: &str,
    md: &MarkdownParser,
    index: &OffsetIndex,
) -> (Vec<SupramarkNode>, Vec<Diagnostic>) {
    // CommonMark: a single leading U+FEFF byte-order mark is ignored at the
    // document start. Skip it from the slice fed to the parser; the base offset
    // keeps source-map byte offsets aligned with the original input (the BOM's
    // 3 UTF-8 bytes are accounted for, not erased).
    let bom_len = if source.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    let children = map_markdown_fragment(md, source, bom_len, source.len(), index);
    let mut diagnostics = Vec::new();
    collect_diagnostics(&children, &mut diagnostics);
    (children, diagnostics)
}

fn collect_diagnostics(nodes: &[SupramarkNode], out: &mut Vec<Diagnostic>) {
    for node in nodes {
        if let SupramarkNode::Unsupported { diagnostics, .. } = node {
            out.extend(diagnostics.iter().cloned());
        }
        visit_children(node, |children| collect_diagnostics(children, out));
    }
}

/// Build the AST v2 node for an extension block (container / input). An unclosed
/// opener becomes an Unsupported node carrying a diagnostic; a closed one
/// dispatches by name into the opaque container/input shapes.
pub(crate) fn build_extension_node(
    open: &ExtensionOpen,
    value: String,
    position: Option<SourcePosition>,
    closed: bool,
) -> SupramarkNode {
    if !closed {
        let diagnostic = Diagnostic {
            code: "unclosed_extension_block".to_owned(),
            severity: DiagnosticSeverity::Error,
            message: format!("Missing closing `{}` marker.", open.close_marker),
            position: position.clone(),
            data: None,
        };
        return SupramarkNode::Unsupported {
            syntax: open.syntax_name().to_owned(),
            reason: "missing closing marker".to_owned(),
            value: Some(value),
            children: Vec::new(),
            diagnostics: vec![diagnostic],
            position,
        };
    }

    match open.syntax {
        ExtensionSyntax::Container => {
            let data = match open.name.as_str() {
                "map" => parse_map_data(&value),
                "vison" => Some(parse_vison_data(&value)),
                "html" => Some(serde_json::json!({ "html": value.clone() })),
                "weather" => Some(parse_weather_data(open.params.as_deref(), &value)),
                _ => None,
            };
            SupramarkNode::Container {
                name: open.name.clone(),
                mode: ExtensionMode::Opaque,
                params: open.params.clone(),
                children: Vec::new(),
                value: Some(value),
                data,
                position,
            }
        }
        ExtensionSyntax::Input => SupramarkNode::Input {
            name: open.name.clone(),
            mode: ExtensionMode::Opaque,
            params: open.params.clone(),
            children: Vec::new(),
            value: Some(value),
            data: None,
            position,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_inline_positions_with_utf16_offsets() {
        // cjk-allow: multi-byte CJK + emoji source needed to exercise utf16-offset math
        let ast = parse("# 标题 😄\n\nHello **世界** and `code`.");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Paragraph {
            children: paragraph_children,
            ..
        } = &children[1]
        else {
            panic!("expected paragraph");
        };

        let SupramarkNode::Strong {
            children: strong_children,
            position,
        } = &paragraph_children[1]
        else {
            panic!("expected strong");
        };

        assert_eq!(position.as_ref().unwrap().start.byte_offset, 21);
        assert_eq!(position.as_ref().unwrap().start.utf16_offset, 15);
        assert!(matches!(strong_children[0], SupramarkNode::Text { .. }));
    }

    #[test]
    fn maps_diagram_fences() {
        for (lang, expected_engine) in [
            ("mermaid", "mermaid"),
            ("graphviz", "graphviz"),
            ("vega", "vega"),
            ("chart", "chart"),
            ("chartjs", "chartjs"),
            ("chart.js", "chart.js"),
        ] {
            let ast = parse(&format!("```{lang}\ngraph TD; A-->B;\n```"));
            let SupramarkNode::Root { children, .. } = ast else {
                panic!("expected root");
            };

            let SupramarkNode::Diagram {
                engine,
                code,
                fence_closed,
                ..
            } = &children[0]
            else {
                panic!("expected diagram for {lang}");
            };

            assert_eq!(engine, expected_engine);
            assert_eq!(code.trim(), "graph TD; A-->B;");
            assert!(fence_closed);
        }
    }

    #[test]
    fn marks_unclosed_diagram_fence_as_open() {
        let ast = parse("```mermaid\ngraph TD; A-->B;");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Diagram { fence_closed, .. } = &children[0] else {
            panic!("expected diagram");
        };

        assert!(!fence_closed);
    }

    #[test]
    fn deserializes_legacy_diagram_without_fence_closed() {
        // Previously persisted ASTs predate the `fence_closed` field; the serde
        // default keeps them deserializable instead of failing, defaulting to
        // the safe "potentially open" value.
        let json = r#"{"type":"diagram","engine":"mermaid","code":"graph TD; A-->B;"}"#;
        let node: SupramarkNode =
            serde_json::from_str(json).expect("legacy AST should deserialize");
        match node {
            SupramarkNode::Diagram { fence_closed, .. } => assert!(!fence_closed),
            _ => panic!("expected diagram"),
        }
    }

    #[test]
    fn maps_plotly_as_unsupported_diagram_fence() {
        let ast = parse("```plotly\n{}\n```");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Diagram { engine, code, .. } = &children[0] else {
            panic!("expected diagram");
        };

        assert_eq!(engine, "plotly");
        assert_eq!(code.trim(), "{}");
    }

    #[test]
    fn maps_gfm_tables() {
        let ast = parse("| A | B |\n|:-|--:|\n| 1 | 2 |\n");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Table {
            align, children, ..
        } = &children[0]
        else {
            panic!("expected table");
        };

        assert_eq!(
            align,
            &vec![Some(TableAlign::Left), Some(TableAlign::Right)]
        );
        assert_eq!(children.len(), 2);
        let SupramarkNode::TableRow {
            children: cells, ..
        } = &children[0]
        else {
            panic!("expected table row");
        };
        let SupramarkNode::TableCell { header, .. } = &cells[0] else {
            panic!("expected table cell");
        };
        assert!(*header);
    }

    #[test]
    fn maps_inline_math_and_footnote_references() {
        let ast = parse("Inline $E = mc^2$ text[^note].");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Paragraph {
            children: paragraph,
            ..
        } = &children[0]
        else {
            panic!("expected paragraph");
        };

        assert!(matches!(
            &paragraph[1],
            SupramarkNode::MathInline { value, .. } if value == "E = mc^2"
        ));
        assert!(matches!(
            &paragraph[3],
            SupramarkNode::FootnoteReference { label, .. } if label == "note"
        ));
    }

    #[test]
    fn reassembles_footnote_ref_split_by_inline_raw_html() {
        // A `[^label]` whose label contains `<` must be tokenized as one
        // footnote reference — the inline raw-HTML rule must not split it,
        // otherwise the embedded tag leaks into the output (XSS). cmark-gfm
        // treats the whole `[…]` as the label string and percent-encodes it.
        let ast = parse("Hello[^\"><script>alert(1)</script>]\n\n[^\"><script>alert(1)</script>]: pwned\n");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::Paragraph {
            children: paragraph,
            ..
        } = &children[0]
        else {
            panic!("expected paragraph");
        };
        let reference = paragraph
            .iter()
            .find_map(|node| match node {
                SupramarkNode::FootnoteReference { .. } => Some(node),
                _ => None,
            })
            .expect("footnote reference");
        match reference {
            SupramarkNode::FootnoteReference {
                label,
                identifier,
                index,
                ..
            } => {
                assert_eq!(label, "\"><script>alert(1)</script>");
                assert_eq!(identifier, "\"><script>alert(1)</script>");
                assert_eq!(*index, 1, "footnote reference should match its definition");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn maps_multiple_inline_math_runs_split_by_text_tokens() {
        let ast = parse("Inline $E = mc^2$ and $A = \\pi r^2$.");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Paragraph {
            children: paragraph,
            ..
        } = &children[0]
        else {
            panic!("expected paragraph");
        };

        let math_values = paragraph
            .iter()
            .filter_map(|node| match node {
                SupramarkNode::MathInline { value, .. } => Some(value.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(math_values, vec!["E = mc^2", "A = \\pi r^2"]);
    }

    #[test]
    fn maps_footnote_definitions() {
        let ast = parse("Body[^1].\n\n[^1]: Footnote body.");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        assert!(matches!(
            &children[1],
            SupramarkNode::FootnoteDefinition { label, children, .. }
                if label == "1" && matches!(children.first(), Some(SupramarkNode::Paragraph { .. }))
        ));
    }

    #[test]
    fn maps_autolink_to_link() {
        // `<https://example.com>` is a CommonMark autolink; the AST must carry
        // the URL on a Link node (not a bare text/raw node) so the web renderer
        // can emit an `<a href="…">`.
        let ast = parse("<https://example.com>");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::Paragraph {
            children: paragraph,
            ..
        } = &children[0]
        else {
            panic!("expected paragraph");
        };
        let SupramarkNode::Link {
            url,
            children: link_children,
            ..
        } = &paragraph[0]
        else {
            panic!("expected link for autolink");
        };
        assert_eq!(url, "https://example.com");
        assert!(matches!(
            link_children.first(),
            Some(SupramarkNode::Text { value, .. }) if value == "https://example.com"
        ));
    }

    /// Walk the paragraph children and return (value, byte-offset span) for
    /// text fragments and autolink links produced by a bare-URL/email split.
    fn autolink_split_spans(input: &str) -> Vec<(String, Option<(usize, usize)>)> {
        let ast = parse(input);
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::Paragraph { children: p, .. } = &children[0] else {
            panic!("expected paragraph");
        };
        p.iter()
            .filter_map(|n| match n {
                SupramarkNode::Text { value, position } => {
                    Some((value.clone(), byte_span(position)))
                }
                SupramarkNode::Link { children, position, .. } => {
                    let text = children
                        .iter()
                        .filter_map(|c| match c {
                            SupramarkNode::Text { value, .. } => Some(value.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    Some((text, byte_span(position)))
                }
                _ => None,
            })
            .collect()
    }

    fn byte_span(position: &Option<SourcePosition>) -> Option<(usize, usize)> {
        position
            .as_ref()
            .map(|p| (p.start.byte_offset, p.end.byte_offset))
    }

    #[test]
    fn bare_email_autolink_preserves_source_positions() {
        // "see foo@bar.baz now" -> Text("see ") | Link("foo@bar.baz") | Text(" now")
        // The original text node's srcmap must be sliced onto each fragment;
        // before the fix every split node carried position: None.
        assert_eq!(
            autolink_split_spans("see foo@bar.baz now"),
            vec![
                ("see ".to_owned(), Some((0, 4))),
                ("foo@bar.baz".to_owned(), Some((4, 15))),
                (" now".to_owned(), Some((15, 19))),
            ]
        );
    }

    #[test]
    fn bare_www_autolink_preserves_source_positions() {
        // "go www.example.com here" -> Text("go ") | Link | Text(" here")
        assert_eq!(
            autolink_split_spans("go www.example.com here"),
            vec![
                ("go ".to_owned(), Some((0, 3))),
                ("www.example.com".to_owned(), Some((3, 18))),
                (" here".to_owned(), Some((18, 23))),
            ]
        );
    }

    #[test]
    fn table_autolink_after_escaped_pipe_preserves_exact_source_positions() {
        let source = "| a |\n| - |\n| x\\| www.example.com |\n";
        let ast = parse(source);
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::Table { children: rows, .. } = &children[0] else {
            panic!("expected table");
        };
        let SupramarkNode::TableRow {
            children: cells, ..
        } = &rows[1]
        else {
            panic!("expected body row");
        };
        let SupramarkNode::TableCell { children, .. } = &cells[0] else {
            panic!("expected body cell");
        };

        let SupramarkNode::Text {
            value,
            position: Some(text_position),
        } = &children[0]
        else {
            panic!("expected positioned text before the link");
        };
        assert_eq!(value, "x| ");
        assert_eq!(
            &source[text_position.start.byte_offset..text_position.end.byte_offset],
            "x\\| "
        );

        let SupramarkNode::Link {
            position: Some(link_position),
            ..
        } = &children[1]
        else {
            panic!("expected positioned autolink");
        };
        assert_eq!(
            &source[link_position.start.byte_offset..link_position.end.byte_offset],
            "www.example.com"
        );
    }

    #[test]
    fn chained_at_skipped_region_keeps_text_position() {
        // `a@a@a@a post b@c.d`: the `a@a@a@a` chain finds no email (no dot),
        // so it is skipped as plain text and the `b@c.d` link is matched.
        // The pre-link text fragment must still carry a contiguous position
        // spanning from the start through the link's start.
        assert_eq!(
            autolink_split_spans("a@a@a@a post b@c.d"),
            vec![
                ("a@a@a@a post ".to_owned(), Some((0, 13))),
                ("b@c.d".to_owned(), Some((13, 18))),
            ]
        );
    }

    #[test]
    fn reassembled_footnote_ref_carries_position() {
        // The `[^…<…>]` footnote reference is reassembled across an inline
        // raw-HTML split; the resulting FootnoteReference (and the text-before
        // / text-after fragments) must carry source positions, not None.
        let ast = parse("Hi[^\"><script>alert(1)</script>]\n\n[^\"><script>alert(1)</script>]: x\n");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::Paragraph { children: p, .. } = &children[0] else {
            panic!("expected paragraph");
        };
        let spans: Vec<Option<(usize, usize)>> = p
            .iter()
            .filter_map(|n| match n {
                SupramarkNode::Text { position, .. } => Some(byte_span(position)),
                SupramarkNode::FootnoteReference { position, .. } => Some(byte_span(position)),
                _ => None,
            })
            .collect();
        assert!(
            spans.iter().all(Option::is_some),
            "no node should lose its position: {spans:?}"
        );
        let r#ref = p
            .iter()
            .find_map(|n| match n {
                SupramarkNode::FootnoteReference { position, .. } => byte_span(position),
                _ => None,
            })
            .expect("reassembled footnote reference");
        // Spans from the `[` (offset 2, after "Hi") to just past the closing `]`.
        assert_eq!(r#ref, (2, 32));
    }

    #[test]
    fn html_block_closing_tag_paragraph_interruption() {
        // CommonMark 0.31.2 HTML block type-6 start condition uses a name list
        // that excludes pre/script/style/textarea (those are type-1 via their
        // *open* tags). So `</td>` (a type-6 name) interrupts the paragraph and
        // becomes its own raw block, while `</pre>` matches no start condition
        // and stays inline within the paragraph.
        let ast = parse("bar\n</td>\n");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        assert_eq!(
            children.len(),
            2,
            "closing td tag should interrupt the paragraph"
        );
        assert!(matches!(&children[0], SupramarkNode::Paragraph { .. }));
        assert!(matches!(
            &children[1],
            SupramarkNode::Raw { block: true, value, .. } if value == "</td>\n"
        ));

        let ast = parse("bar\n</pre>\n");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        assert_eq!(
            children.len(),
            1,
            "closing pre tag must NOT interrupt the paragraph"
        );
        let SupramarkNode::Paragraph {
            children: paragraph,
            ..
        } = &children[0]
        else {
            panic!("expected paragraph");
        };
        assert!(paragraph
            .iter()
            .any(|node| matches!(node, SupramarkNode::Raw { value, .. } if value == "</pre>")));
    }

    #[test]
    fn unescapes_code_fence_info_string() {
        // CommonMark unescapes backslash escapes and entities in the fence info
        // string, so ````js&amp;`` / ````js\!`` resolve to `js&` / `js!` rather
        // than being passed through literally.
        for (input, expected_lang) in [("```js&amp;\nx\n```", "js&"), ("```js\\!\nx\n```", "js!")] {
            let ast = parse(input);
            let SupramarkNode::Root { children, .. } = ast else {
                panic!("expected root");
            };
            let SupramarkNode::Code { lang, .. } = &children[0] else {
                panic!("expected code block for {input}");
            };
            assert_eq!(lang.as_deref(), Some(expected_lang));
        }
    }

    #[test]
    fn task_marker_consumes_separator_whitespace() {
        // cmark-gfm's tasklist scan consumes the marker plus its trailing
        // `spacechar+` separator; the text node carries the content with no
        // leading whitespace, regardless of whether the source separator was a
        // space, a tab, or several spaces. Previously the raw separator was
        // preserved, leaving `\tfoo` / `  foo` in the text node.
        fn first_text(nodes: &[SupramarkNode]) -> &str {
            match &nodes[0] {
                SupramarkNode::Paragraph { children, .. } => first_text(children),
                SupramarkNode::Text { value, .. } => value,
                _ => panic!("expected text"),
            }
        }
        for (input, expected_text) in [
            ("- [ ] foo", "foo"),
            ("- [ ]\tfoo", "foo"),
            ("- [ ]  foo", "foo"),
            ("- [x] bar", "bar"),
        ] {
            let ast = parse(input);
            let SupramarkNode::Root { children, .. } = ast else {
                panic!("expected root for {input}");
            };
            let SupramarkNode::List { children: items, .. } = &children[0] else {
                panic!("expected list for {input}");
            };
            let SupramarkNode::ListItem { children, .. } = &items[0] else {
                panic!("expected list item for {input}");
            };
            assert_eq!(first_text(children), expected_text, "for input {input}");
        }
    }

    #[test]
    fn task_marker_requires_separator_whitespace() {
        // cmark-gfm does NOT treat `[ ]foo` (no separator) as a task list
        // item; the marker must be followed by `spacechar+`. The text node
        // stays literal.
        fn first_text(nodes: &[SupramarkNode]) -> &str {
            match &nodes[0] {
                SupramarkNode::Paragraph { children, .. } => first_text(children),
                SupramarkNode::Text { value, .. } => value,
                _ => panic!("expected text"),
            }
        }
        let ast = parse("- [ ]foo");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };
        let SupramarkNode::List { children: items, .. } = &children[0] else {
            panic!("expected list");
        };
        let SupramarkNode::ListItem { checked, children, .. } = &items[0] else {
            panic!("expected list item");
        };
        assert!(checked.is_none(), "no-separator should not be a task item");
        assert_eq!(first_text(children), "[ ]foo");
    }
}
