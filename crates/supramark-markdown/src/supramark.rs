use crate::plugins::cmark::block::blockquote::Blockquote;
use crate::plugins::cmark::block::code::CodeBlock;
use crate::plugins::cmark::block::fence::CodeFence;
use crate::plugins::cmark::block::heading::ATXHeading;
use crate::plugins::cmark::block::hr::ThematicBreak;
use crate::plugins::cmark::block::lheading::SetextHeader;
use crate::plugins::cmark::block::list::{BulletList, ListItem, OrderedList};
use crate::plugins::cmark::block::paragraph::Paragraph;
use crate::plugins::cmark::inline::backticks::CodeInline;
use crate::plugins::cmark::inline::emphasis::{Em, Strong};
use crate::plugins::cmark::inline::image::Image;
use crate::plugins::cmark::inline::link::Link;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::extra::strikethrough::Strikethrough;
use crate::plugins::extra::tables::{
    ColumnAlignment, Table, TableBody, TableCell, TableHead, TableRow,
};
use crate::{MarkdownIt, Node};
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
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
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
        children: Vec<SupramarkNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<SourcePosition>,
    },
    FootnoteReference {
        index: u32,
        label: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParseOptions {
    gfm_tables: bool,
    gfm_strikethrough: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            gfm_tables: true,
            gfm_strikethrough: true,
        }
    }
}

pub fn parse(source: &str) -> SupramarkNode {
    parse_with_options(source, ParseOptions::default())
}

fn parse_with_options(source: &str, options: ParseOptions) -> SupramarkNode {
    let md = create_parser(options);
    let index = OffsetIndex::new(source);
    let (mut children, diagnostics) = map_document(source, &md, &index);
    assign_footnote_indices(&mut children);
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

fn create_parser(options: ParseOptions) -> MarkdownIt {
    let mut md = MarkdownIt::new();
    crate::plugins::cmark::add(&mut md);

    if options.gfm_tables {
        crate::plugins::extra::tables::add(&mut md);
    }
    if options.gfm_strikethrough {
        crate::plugins::extra::strikethrough::add(&mut md);
    }

    md
}

fn map_document(
    source: &str,
    md: &MarkdownIt,
    index: &OffsetIndex,
) -> (Vec<SupramarkNode>, Vec<Diagnostic>) {
    let lines = LineSpan::scan(source);
    if lines.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut children = Vec::new();
    let mut diagnostics = Vec::new();
    let mut normal_start = 0;
    let mut line = 0;

    while line < lines.len() {
        if let Some((node, next_line)) = map_math_block(source, &lines, line, index) {
            if normal_start < line {
                children.extend(map_markdown_fragment(
                    md,
                    source,
                    lines[normal_start].start,
                    lines[line].start,
                    index,
                ));
            }

            children.push(node);
            line = next_line;
            normal_start = line;
            continue;
        }

        if let Some((node, next_line)) = map_footnote_definition(source, md, &lines, line, index) {
            if normal_start < line {
                children.extend(map_markdown_fragment(
                    md,
                    source,
                    lines[normal_start].start,
                    lines[line].start,
                    index,
                ));
            }

            children.push(node);
            line = next_line;
            normal_start = line;
            continue;
        }

        if let Some((node, next_line)) = map_definition_list(source, md, &lines, line, index) {
            if normal_start < line {
                children.extend(map_markdown_fragment(
                    md,
                    source,
                    lines[normal_start].start,
                    lines[line].start,
                    index,
                ));
            }

            children.push(node);
            line = next_line;
            normal_start = line;
            continue;
        }

        if let Some(open) = parse_extension_open(lines[line].text) {
            if normal_start < line {
                children.extend(map_markdown_fragment(
                    md,
                    source,
                    lines[normal_start].start,
                    lines[line].start,
                    index,
                ));
            }

            match find_closing_line(&lines, line + 1, open.close_marker) {
                Some(close_line) => {
                    let node_start = lines[line].start;
                    let node_end = lines[close_line].end_with_newline;
                    let value = join_line_text(&lines[line + 1..close_line]);
                    children.push(map_extension_block(
                        open,
                        value,
                        SourcePosition {
                            start: index.point_at(node_start),
                            end: index.point_at(node_end),
                        },
                    ));
                    line = close_line + 1;
                    normal_start = line;
                    continue;
                }
                None => {
                    let node_start = lines[line].start;
                    let node_end = source.len();
                    let position = SourcePosition {
                        start: index.point_at(node_start),
                        end: index.point_at(node_end),
                    };
                    let diagnostic = Diagnostic {
                        code: "unclosed_extension_block".to_owned(),
                        severity: DiagnosticSeverity::Error,
                        message: format!("Missing closing `{}` marker.", open.close_marker),
                        position: Some(position.clone()),
                        data: None,
                    };
                    diagnostics.push(diagnostic.clone());
                    children.push(SupramarkNode::Unsupported {
                        syntax: open.syntax_name().to_owned(),
                        reason: "missing closing marker".to_owned(),
                        value: Some(source[node_start..node_end].to_owned()),
                        children: Vec::new(),
                        diagnostics: vec![diagnostic],
                        position: Some(position),
                    });
                    return (children, diagnostics);
                }
            }
        }

        if is_raw_html_line(lines[line].text) {
            if normal_start < line {
                children.extend(map_markdown_fragment(
                    md,
                    source,
                    lines[normal_start].start,
                    lines[line].start,
                    index,
                ));
            }

            children.push(SupramarkNode::Raw {
                format: "html".to_owned(),
                value: lines[line].text.trim().to_owned(),
                block: true,
                position: Some(SourcePosition {
                    start: index.point_at(lines[line].start),
                    end: index.point_at(lines[line].end_with_newline),
                }),
            });
            line += 1;
            normal_start = line;
            continue;
        }

        line += 1;
    }

    if normal_start < lines.len() {
        children.extend(map_markdown_fragment(
            md,
            source,
            lines[normal_start].start,
            source.len(),
            index,
        ));
    }

    (children, diagnostics)
}

fn map_markdown_fragment(
    md: &MarkdownIt,
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

fn map_children(children: &[Node], index: &OffsetIndex, base_offset: usize) -> Vec<SupramarkNode> {
    children
        .iter()
        .flat_map(|child| map_node(child, index, base_offset))
        .collect()
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
            SupramarkNode::FootnoteReference { label, .. } => {
                assign_footnote_label(label, labels, next_index);
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
                label, children, ..
            } => {
                assign_footnote_label(label, labels, next_index);
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
            SupramarkNode::FootnoteReference { index, label, .. }
            | SupramarkNode::FootnoteDefinition { index, label, .. } => {
                *index = labels.get(label).copied().unwrap_or(0);
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
    let position = position_for(node, index, base_offset);

    if let Some(text) = node.cast::<crate::parser::inline::Text>() {
        return map_inline_text(&text.content, position, index);
    }

    if let Some(text) = node.cast::<crate::parser::inline::TextSpecial>() {
        return map_inline_text(&text.content, position, index);
    }

    if node.is::<Paragraph>() {
        return vec![SupramarkNode::Paragraph {
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if let Some(heading) = node.cast::<ATXHeading>() {
        return vec![SupramarkNode::Heading {
            depth: heading.level,
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if let Some(heading) = node.cast::<SetextHeader>() {
        return vec![SupramarkNode::Heading {
            depth: heading.level,
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<Strong>() {
        return vec![SupramarkNode::Strong {
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<Em>() {
        return vec![SupramarkNode::Emphasis {
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<CodeInline>() {
        return vec![SupramarkNode::InlineCode {
            value: node.collect_text(),
            position,
        }];
    }

    if let Some(link) = node.cast::<Link>() {
        return vec![SupramarkNode::Link {
            url: link.url.clone(),
            title: link.title.clone(),
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if let Some(image) = node.cast::<Image>() {
        return vec![SupramarkNode::Image {
            url: image.url.clone(),
            title: image.title.clone(),
            alt: node.collect_text(),
            position,
        }];
    }

    if node.is::<Hardbreak>() {
        return vec![SupramarkNode::Break { position }];
    }

    if node.is::<Softbreak>() {
        return vec![SupramarkNode::Text {
            value: "\n".to_owned(),
            position,
        }];
    }

    if node.is::<Strikethrough>() {
        return vec![SupramarkNode::Delete {
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if let Some(code) = node.cast::<CodeBlock>() {
        return vec![SupramarkNode::Code {
            value: code.content.clone(),
            lang: None,
            meta: None,
            position,
        }];
    }

    if let Some(fence) = node.cast::<CodeFence>() {
        return vec![map_fence(fence, position)];
    }

    if let Some(list) = node.cast::<OrderedList>() {
        return vec![SupramarkNode::List {
            ordered: true,
            start: Some(list.start),
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<BulletList>() {
        return vec![SupramarkNode::List {
            ordered: false,
            start: None,
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<ListItem>() {
        let (checked, children) = map_list_item_children(&node.children, index, base_offset);
        return vec![SupramarkNode::ListItem {
            checked,
            children,
            position,
        }];
    }

    if node.is::<Blockquote>() {
        return vec![SupramarkNode::Blockquote {
            children: map_children(&node.children, index, base_offset),
            position,
        }];
    }

    if node.is::<ThematicBreak>() {
        return vec![SupramarkNode::ThematicBreak { position }];
    }

    if let Some(table) = node.cast::<Table>() {
        return vec![SupramarkNode::Table {
            align: table.alignments.iter().map(map_alignment).collect(),
            children: map_table_sections(&node.children, &table.alignments, index, base_offset),
            position,
        }];
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
                nodes.push(SupramarkNode::FootnoteReference {
                    index: 0,
                    label: value[label_start..end].to_owned(),
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
        let Some((relative, ch)) = chars.next() else {
            return None;
        };
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
    if !value.contains(':') {
        return value.to_owned();
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
    let rest = rest.strip_prefix([' ', '\t']).unwrap_or(rest);
    let mut replacement = String::new();
    replacement.push_str(&value[..leading_len]);
    replacement.push_str(rest);
    *value = replacement;
    Some(checked)
}

fn map_fence(fence: &CodeFence, position: Option<SourcePosition>) -> SupramarkNode {
    let info = fence.info.trim();
    let mut parts = info.split_whitespace();
    let lang = parts.next().map(str::to_owned);
    let meta = {
        let rest = parts.collect::<Vec<_>>().join(" ");
        (!rest.is_empty()).then_some(rest)
    };

    if let Some(engine) = lang.as_deref().and_then(diagram_engine) {
        SupramarkNode::Diagram {
            engine: engine.to_owned(),
            code: fence.content.clone(),
            meta,
            position,
        }
    } else {
        SupramarkNode::Code {
            value: fence.content.clone(),
            lang,
            meta,
            position,
        }
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

fn map_alignment(alignment: &ColumnAlignment) -> Option<TableAlign> {
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
struct LineSpan<'a> {
    start: usize,
    end_with_newline: usize,
    text: &'a str,
}

impl<'a> LineSpan<'a> {
    fn scan(source: &'a str) -> Vec<Self> {
        let mut lines = Vec::new();
        let mut start = 0;

        while start < source.len() {
            let relative_newline = source[start..].find('\n');
            let (end_no_newline, end_with_newline) = match relative_newline {
                Some(relative) => {
                    let newline = start + relative;
                    let end_no_newline =
                        if newline > start && source.as_bytes()[newline - 1] == b'\r' {
                            newline - 1
                        } else {
                            newline
                        };
                    (end_no_newline, newline + 1)
                }
                None => (source.len(), source.len()),
            };

            lines.push(Self {
                start,
                end_with_newline,
                text: &source[start..end_no_newline],
            });

            if end_with_newline == source.len() {
                break;
            }
            start = end_with_newline;
        }

        lines
    }
}

#[derive(Debug, Clone, Copy)]
enum ExtensionSyntax {
    Container,
    Input,
}

#[derive(Debug, Clone)]
struct ExtensionOpen {
    syntax: ExtensionSyntax,
    name: String,
    params: Option<String>,
    close_marker: &'static str,
}

impl ExtensionOpen {
    fn syntax_name(&self) -> &'static str {
        match self.syntax {
            ExtensionSyntax::Container => "container",
            ExtensionSyntax::Input => "input",
        }
    }
}

fn parse_extension_open(line: &str) -> Option<ExtensionOpen> {
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

fn find_closing_line(lines: &[LineSpan<'_>], start: usize, close_marker: &str) -> Option<usize> {
    lines[start..]
        .iter()
        .position(|line| line.text.trim() == close_marker)
        .map(|offset| start + offset)
}

fn join_line_text(lines: &[LineSpan<'_>]) -> String {
    lines
        .iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n")
}

fn map_math_block(
    _source: &str,
    lines: &[LineSpan<'_>],
    start_line: usize,
    index: &OffsetIndex,
) -> Option<(SupramarkNode, usize)> {
    if lines[start_line].text.trim() != "$$" {
        return None;
    }

    let close_line = lines[start_line + 1..]
        .iter()
        .position(|line| line.text.trim() == "$$")
        .map(|offset| start_line + 1 + offset)?;

    let value = join_line_text(&lines[start_line + 1..close_line]);
    let position = SourcePosition {
        start: index.point_at(lines[start_line].start),
        end: index.point_at(lines[close_line].end_with_newline),
    };

    Some((
        SupramarkNode::MathBlock {
            value,
            position: Some(position),
        },
        close_line + 1,
    ))
}

fn map_footnote_definition(
    source: &str,
    md: &MarkdownIt,
    lines: &[LineSpan<'_>],
    start_line: usize,
    index: &OffsetIndex,
) -> Option<(SupramarkNode, usize)> {
    let line = &lines[start_line];
    let (label, content_start) = parse_footnote_definition_line(line)?;
    let content_end = line.start + line.text.len();
    let children = if content_start < content_end {
        map_markdown_fragment(md, source, content_start, content_end, index)
    } else {
        Vec::new()
    };

    Some((
        SupramarkNode::FootnoteDefinition {
            index: 0,
            label,
            children,
            position: Some(SourcePosition {
                start: index.point_at(line.start),
                end: index.point_at(line.end_with_newline),
            }),
        },
        start_line + 1,
    ))
}

fn parse_footnote_definition_line(line: &LineSpan<'_>) -> Option<(String, usize)> {
    let leading = leading_whitespace_len(line.text);
    if leading > 3 {
        return None;
    }

    let rest = &line.text[leading..];
    let label_rest = rest.strip_prefix("[^")?;
    let close = label_rest.find("]:")?;
    let label = &label_rest[..close];
    if label.is_empty() {
        return None;
    }

    let mut content_relative = leading + 2 + close + 2;
    let content_rest = &line.text[content_relative..];
    content_relative += leading_whitespace_len(content_rest);

    Some((label.to_owned(), line.start + content_relative))
}

fn map_definition_list(
    source: &str,
    md: &MarkdownIt,
    lines: &[LineSpan<'_>],
    start_line: usize,
    index: &OffsetIndex,
) -> Option<(SupramarkNode, usize)> {
    let mut cursor = start_line;
    let mut items = Vec::new();
    let mut list_end = lines[start_line].start;

    while cursor < lines.len() {
        let term_start = cursor;
        let mut term_lines = Vec::new();

        while cursor < lines.len() && is_definition_term_line(lines[cursor].text) {
            term_lines.push(cursor);
            cursor += 1;
        }

        if term_lines.is_empty()
            || cursor >= lines.len()
            || !is_definition_description_line(lines[cursor].text)
        {
            if items.is_empty() {
                return None;
            }
            break;
        }

        let mut item_children = Vec::new();
        for term_line in &term_lines {
            let line = &lines[*term_line];
            let content_start = line.start + leading_whitespace_len(line.text);
            let content_end = line.start + line.text.len();
            item_children.push(SupramarkNode::DefinitionTerm {
                children: map_markdown_fragment_as_inline(
                    md,
                    source,
                    content_start,
                    content_end,
                    index,
                ),
                position: Some(SourcePosition {
                    start: index.point_at(line.start),
                    end: index.point_at(line.end_with_newline),
                }),
            });
        }

        while cursor < lines.len() && is_definition_description_line(lines[cursor].text) {
            let line = &lines[cursor];
            let content_start = definition_description_content_start(line);
            let content_end = line.start + line.text.len();
            let children = if content_start < content_end {
                map_markdown_fragment(md, source, content_start, content_end, index)
            } else {
                Vec::new()
            };
            item_children.push(SupramarkNode::DefinitionDescription {
                children,
                position: Some(SourcePosition {
                    start: index.point_at(line.start),
                    end: index.point_at(line.end_with_newline),
                }),
            });
            list_end = line.end_with_newline;
            cursor += 1;
        }

        items.push(SupramarkNode::DefinitionItem {
            children: item_children,
            position: Some(SourcePosition {
                start: index.point_at(lines[term_start].start),
                end: index.point_at(list_end),
            }),
        });

        if cursor >= lines.len() || lines[cursor].text.trim().is_empty() {
            break;
        }
    }

    Some((
        SupramarkNode::DefinitionList {
            children: items,
            position: Some(SourcePosition {
                start: index.point_at(lines[start_line].start),
                end: index.point_at(list_end),
            }),
        },
        cursor,
    ))
}

fn map_markdown_fragment_as_inline(
    md: &MarkdownIt,
    source: &str,
    start: usize,
    end: usize,
    index: &OffsetIndex,
) -> Vec<SupramarkNode> {
    let mut blocks = map_markdown_fragment(md, source, start, end, index);
    if blocks.len() == 1 {
        if let SupramarkNode::Paragraph { children, .. } = blocks.remove(0) {
            return children;
        }
    }
    blocks
}

fn is_definition_term_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && !is_definition_description_line(line)
        && parse_extension_open(line).is_none()
        && !is_raw_html_line(line)
        && trimmed != "$$"
}

fn is_definition_description_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix(':')
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn leading_whitespace_len(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn definition_description_content_start(line: &LineSpan<'_>) -> usize {
    let leading = leading_whitespace_len(line.text);
    let after_colon = leading + 1;
    let rest = &line.text[after_colon..];
    line.start + after_colon + leading_whitespace_len(rest)
}

fn map_extension_block(
    open: ExtensionOpen,
    value: String,
    position: SourcePosition,
) -> SupramarkNode {
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
                name: open.name,
                mode: ExtensionMode::Opaque,
                params: open.params,
                children: Vec::new(),
                value: Some(value),
                data,
                position: Some(position),
            }
        }
        ExtensionSyntax::Input => SupramarkNode::Input {
            name: open.name,
            mode: ExtensionMode::Opaque,
            params: open.params,
            children: Vec::new(),
            value: Some(value),
            data: None,
            position: Some(position),
        },
    }
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

fn is_raw_html_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("<!--") || trimmed.starts_with("<!") {
        return true;
    }
    let mut chars = trimmed.chars();
    if chars.next() != Some('<') {
        return false;
    }
    matches!(chars.next(), Some('a'..='z' | 'A'..='Z' | '/'))
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_inline_positions_with_utf16_offsets() {
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
        let ast = parse("```mermaid\ngraph TD; A-->B;\n```");
        let SupramarkNode::Root { children, .. } = ast else {
            panic!("expected root");
        };

        let SupramarkNode::Diagram { engine, code, .. } = &children[0] else {
            panic!("expected diagram");
        };

        assert_eq!(engine, "mermaid");
        assert_eq!(code.trim(), "graph TD; A-->B;");
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
}
