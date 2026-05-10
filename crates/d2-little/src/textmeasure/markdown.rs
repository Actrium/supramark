//! Markdown layout for d2.
//!
//! Hosts the recursive HTML-tree walker [`measure_node`] together with the
//! markdown-specific layout constants that were originally inlined in
//! [`super::d2_go_emulation`]. The entry point [`measure_markdown_generic`]
//! drives [`D2GoEmulationRuler`] directly — d2 layout has only one
//! text-measurement backend because `line_height_factor` is a stateful
//! d2-internal concept that other backends cannot cleanly substitute.
//!
//! [`D2GoEmulationRuler::measure_markdown`] is responsible for setting the
//! per-call state (`bounds_with_dot`, `line_height_factor`) before/after
//! delegating into here.

use super::d2_go_emulation::D2GoEmulationRuler;
use crate::fonts::{FontFamily, FontStyle};
use roxmltree::{Document, Node, NodeType};

pub(super) const MARKDOWN_LINE_HEIGHT: f64 = 1.5;

const PADDING_LEFT_UL_OL_EM: f64 = 2.0;
const MARGIN_BOTTOM_UL: f64 = 16.0;

const MARGIN_TOP_LI_P: f64 = 16.0;
const MARGIN_TOP_LI_EM: f64 = 0.25;
const MARGIN_BOTTOM_P: f64 = 16.0;

const LINE_HEIGHT_H: f64 = 1.25;
const MARGIN_TOP_H: f64 = 24.0;
const MARGIN_BOTTOM_H: f64 = 16.0;
const PADDING_BOTTOM_H1_H2_EM: f64 = 0.3;
const BORDER_BOTTOM_H1_H2: f64 = 1.0;

const HEIGHT_HR_EM: f64 = 0.25;
const MARGIN_TOP_BOTTOM_HR: f64 = 24.0;

const PADDING_PRE: f64 = 16.0;
const MARGIN_BOTTOM_PRE: f64 = 16.0;
const LINE_HEIGHT_PRE: f64 = 1.45;
const FONT_SIZE_PRE_CODE_EM: f64 = 0.85;

const PADDING_TOP_BOTTOM_CODE_EM: f64 = 0.2;
const PADDING_LEFT_RIGHT_CODE_EM: f64 = 0.4;

const PADDING_LR_BLOCKQUOTE_EM: f64 = 1.0;
const MARGIN_BOTTOM_BLOCKQUOTE: f64 = 16.0;
const BORDER_LEFT_BLOCKQUOTE_EM: f64 = 0.25;

/// Render `md_text` to sanitised HTML, walk it as a block tree, and return
/// the rendered (width, height) in pixels.
///
/// Per-call state (`bounds_with_dot`, `line_height_factor`) must be set by
/// [`D2GoEmulationRuler::measure_markdown`] before calling in here, and
/// restored after.
pub(super) fn measure_markdown_generic(
    ruler: &mut D2GoEmulationRuler,
    md_text: &str,
    font_family: Option<FontFamily>,
    mono_font_family: Option<FontFamily>,
    font_size: i32,
) -> Result<(i32, i32), String> {
    let render = super::render_markdown(md_text)?;
    let wrapped = format!("<body>{}</body>", render);
    let doc = Document::parse(&wrapped).map_err(|e| format!("markdown parse failed: {e}"))?;

    let body_node = doc.root_element();
    let body_attrs = measure_node(
        ruler,
        0,
        body_node,
        font_family,
        mono_font_family,
        font_size,
        FontStyle::Regular,
    );

    Ok((
        body_attrs.width.ceil() as i32,
        body_attrs.height.ceil() as i32,
    ))
}

#[derive(Debug, Clone, Default)]
struct BlockAttrs {
    width: f64,
    height: f64,
    margin_top: f64,
    margin_bottom: f64,
    extra_data: ExtraData,
}

impl BlockAttrs {
    fn is_not_empty(&self) -> bool {
        self.width != 0.0
            || self.height != 0.0
            || self.margin_top != 0.0
            || self.margin_bottom != 0.0
            || !matches!(self.extra_data, ExtraData::None)
    }
}

#[derive(Debug, Clone, Default)]
enum ExtraData {
    #[default]
    None,
    Row(Vec<f64>),
    Section(Vec<Vec<f64>>),
}

fn trim_markdown_text_node(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, '\n' | '\t' | '\u{0008}'))
}

fn is_empty_sibling_node(node: Node<'_, '_>) -> bool {
    match node.node_type() {
        NodeType::Text => node.text().unwrap_or_default().trim().is_empty(),
        _ => false,
    }
}

fn has_prev(node: Node<'_, '_>) -> bool {
    let Some(prev) = node.prev_sibling() else {
        return false;
    };
    if is_empty_sibling_node(prev) {
        return has_prev(prev);
    }
    true
}

fn has_next(node: Node<'_, '_>) -> bool {
    let Some(next) = node.next_sibling() else {
        return false;
    };
    if is_empty_sibling_node(next) {
        return has_next(next);
    }
    true
}

fn get_prev<'a, 'input>(node: Option<Node<'a, 'input>>) -> Option<Node<'a, 'input>> {
    let node = node?;
    if is_empty_sibling_node(node)
        && let Some(next) = get_next(node.prev_sibling())
    {
        return Some(next);
    }
    Some(node)
}

fn get_next<'a, 'input>(node: Option<Node<'a, 'input>>) -> Option<Node<'a, 'input>> {
    let node = node?;
    if is_empty_sibling_node(node)
        && let Some(next) = get_next(node.next_sibling())
    {
        return Some(next);
    }
    Some(node)
}

fn is_block_element(el_type: &str) -> bool {
    matches!(
        el_type,
        "blockquote"
            | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "ul"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
    )
}

fn has_ancestor_element(node: Node<'_, '_>, el_type: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.is_element() && parent.tag_name().name() == el_type {
        return true;
    }
    has_ancestor_element(parent, el_type)
}

fn merge_column_widths(mut existing: Vec<f64>, new_rows: &[Vec<f64>]) -> Vec<f64> {
    for row_widths in new_rows {
        for (i, width) in row_widths.iter().enumerate() {
            if i >= existing.len() {
                existing.push(*width);
            } else {
                existing[i] = existing[i].max(*width);
            }
        }
    }
    existing
}

fn measure_node(
    ruler: &mut D2GoEmulationRuler,
    depth: usize,
    node: Node<'_, '_>,
    font_family: Option<FontFamily>,
    mono_font_family: Option<FontFamily>,
    mut font_size: i32,
    mut font_style: FontStyle,
) -> BlockAttrs {
    let _ = depth;
    let mut font_family = font_family.unwrap_or(FontFamily::SourceSansPro);

    let parent_element_type = node
        .parent()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name());

    match node.node_type() {
        NodeType::Text => {
            let Some(raw) = node.text() else {
                return BlockAttrs::default();
            };
            if trim_markdown_text_node(raw).is_empty() {
                return BlockAttrs::default();
            }

            let is_code = matches!(parent_element_type, Some("pre" | "code"));
            let font = font_family.font(font_size, font_style);
            let mut str_ = raw.to_owned();
            let mut space_widths = 0.0;

            if !is_code {
                let space_width = ruler.space_width(font);
                str_ = str_.replace('\n', " ");
                str_ = str_.replace('\t', " ");
                if str_.starts_with(' ') {
                    str_.remove(0);
                    if has_prev(node) {
                        space_widths += space_width;
                    }
                }
                if str_.ends_with(' ') {
                    str_.pop();
                    if has_next(node) {
                        space_widths += space_width;
                    }
                }
            }

            if parent_element_type == Some("pre") {
                let original_line_height = ruler.line_height_factor();
                ruler.set_line_height_factor(LINE_HEIGHT_PRE);
                let (mut w, mut h) = ruler.measure_precise(font, &str_);
                ruler.set_line_height_factor(original_line_height);
                w *= FONT_SIZE_PRE_CODE_EM;
                h *= FONT_SIZE_PRE_CODE_EM;
                return BlockAttrs {
                    width: w + space_widths,
                    height: h,
                    ..Default::default()
                };
            }

            let (mut w, h) = ruler.measure_precise(font, &str_);
            if is_code {
                w *= FONT_SIZE_PRE_CODE_EM;
                return BlockAttrs {
                    width: w + space_widths,
                    height: h * FONT_SIZE_PRE_CODE_EM,
                    ..Default::default()
                };
            }

            w = ruler.scale_unicode(w, font, &str_);
            BlockAttrs {
                width: w + space_widths,
                height: h,
                ..Default::default()
            }
        }
        NodeType::Element => {
            let tag = node.tag_name().name();
            let mut is_code = false;

            match tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    font_size = super::header_to_font_size(font_size, tag);
                    font_style = FontStyle::Semibold;
                }
                "em" => {
                    font_style = FontStyle::Italic;
                }
                "b" | "strong" => {
                    font_style = FontStyle::Bold;
                }
                "pre" | "code" => {
                    font_family = mono_font_family.unwrap_or(FontFamily::SourceCodePro);
                    font_style = FontStyle::Regular;
                    is_code = true;
                }
                _ => {}
            }

            let original_line_height = ruler.line_height_factor();
            if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                ruler.set_line_height_factor(LINE_HEIGHT_H);
            }

            let line_height_px = f64::from(font_size) * ruler.line_height_factor();
            let mut block = BlockAttrs::default();

            if node.first_child().is_some() {
                let first = get_next(node.first_child());
                let last = get_prev(node.last_child());
                let mut blocks: Vec<BlockAttrs> = Vec::new();
                let mut inline_block: Option<BlockAttrs> = None;

                let end_inline_block =
                    |blocks: &mut Vec<BlockAttrs>, inline_block: &mut Option<BlockAttrs>| {
                        if let Some(mut b) = inline_block.take() {
                            if !is_code && b.height > 0.0 && b.height < line_height_px {
                                b.height = line_height_px;
                            }
                            blocks.push(b);
                        }
                    };

                for child in node.children() {
                    let child_block = measure_node(
                        ruler,
                        depth + 1,
                        child,
                        Some(font_family),
                        mono_font_family,
                        font_size,
                        font_style,
                    );

                    if child.is_element() && is_block_element(child.tag_name().name()) {
                        end_inline_block(&mut blocks, &mut inline_block);
                        let mut new_block = BlockAttrs {
                            width: child_block.width,
                            height: child_block.height,
                            ..Default::default()
                        };
                        new_block.margin_top = if first == Some(child) && tag == "blockquote" {
                            0.0
                        } else {
                            child_block.margin_top
                        };
                        new_block.margin_bottom = if last == Some(child) && tag == "blockquote" {
                            0.0
                        } else {
                            child_block.margin_bottom
                        };
                        blocks.push(new_block);
                    } else if child.is_element() && child.tag_name().name() == "br" {
                        if inline_block.is_some() {
                            end_inline_block(&mut blocks, &mut inline_block);
                        } else {
                            block.height += line_height_px;
                        }
                    } else if child_block.is_not_empty() {
                        if let Some(ref mut inline) = inline_block {
                            inline.width += child_block.width;
                            inline.height = inline.height.max(child_block.height);
                            inline.margin_top = inline.margin_top.max(child_block.margin_top);
                            inline.margin_bottom =
                                inline.margin_bottom.max(child_block.margin_bottom);
                        } else {
                            inline_block = Some(child_block);
                        }
                    }
                }

                if inline_block.is_some() {
                    end_inline_block(&mut blocks, &mut inline_block);
                }

                let mut prev_margin_bottom = 0.0;
                for (i, b) in blocks.iter().enumerate() {
                    if i == 0 {
                        block.margin_top = block.margin_top.max(b.margin_top);
                    } else {
                        let margin_diff = b.margin_top - prev_margin_bottom;
                        if margin_diff > 0.0 {
                            block.height += margin_diff;
                        }
                    }
                    if i == blocks.len() - 1 {
                        block.margin_bottom = block.margin_bottom.max(b.margin_bottom);
                    } else {
                        block.height += b.margin_bottom;
                        prev_margin_bottom = b.margin_bottom;
                    }

                    block.height += b.height;
                    block.width = block.width.max(b.width);
                }
            }

            match tag {
                "blockquote" => {
                    block.width += (2.0 * PADDING_LR_BLOCKQUOTE_EM + BORDER_LEFT_BLOCKQUOTE_EM)
                        * f64::from(font_size);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_BLOCKQUOTE);
                }
                "p" => {
                    if parent_element_type == Some("li") {
                        block.margin_top = block.margin_top.max(MARGIN_TOP_LI_P);
                    }
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_P);
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    block.margin_top = block.margin_top.max(MARGIN_TOP_H);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_H);
                    if matches!(tag, "h1" | "h2") {
                        block.height +=
                            PADDING_BOTTOM_H1_H2_EM * f64::from(font_size) + BORDER_BOTTOM_H1_H2;
                    }
                }
                "li" => {
                    block.width += PADDING_LEFT_UL_OL_EM * f64::from(font_size);
                    if has_prev(node) {
                        block.margin_top = block
                            .margin_top
                            .max(MARGIN_TOP_LI_EM * f64::from(font_size));
                    }
                }
                "ol" | "ul" => {
                    if has_ancestor_element(node, "ul") || has_ancestor_element(node, "ol") {
                        block.margin_top = 0.0;
                        block.margin_bottom = 0.0;
                    } else {
                        block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_UL);
                    }
                }
                "pre" => {
                    block.width += 2.0 * PADDING_PRE;
                    block.height += 2.0 * PADDING_PRE;
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_PRE);
                }
                "code" if parent_element_type != Some("pre") => {
                    block.width += 2.0 * PADDING_LEFT_RIGHT_CODE_EM * f64::from(font_size);
                    block.height += 2.0 * PADDING_TOP_BOTTOM_CODE_EM * f64::from(font_size);
                }
                "hr" => {
                    block.height += HEIGHT_HR_EM * f64::from(font_size);
                    block.margin_top = block.margin_top.max(MARGIN_TOP_BOTTOM_HR);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_TOP_BOTTOM_HR);
                }
                "table" => {
                    let mut column_widths: Vec<f64> = Vec::new();
                    let mut table_height = 0.0;
                    let table_border = 1.0;

                    for child in node.children() {
                        if child.is_element()
                            && matches!(child.tag_name().name(), "tbody" | "thead" | "tfoot")
                        {
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            table_height += child_attrs.height;
                            if let ExtraData::Section(ref widths) = child_attrs.extra_data {
                                column_widths = merge_column_widths(column_widths, widths);
                            }
                        } else if child.is_element() && child.tag_name().name() == "tr" {
                            let row_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            table_height += row_attrs.height;
                            if let ExtraData::Row(ref widths) = row_attrs.extra_data {
                                column_widths = merge_column_widths(
                                    column_widths,
                                    std::slice::from_ref(widths),
                                );
                            }
                        }
                    }

                    let mut table_width = 0.0;
                    if !column_widths.is_empty() {
                        for col_width in &column_widths {
                            table_width += *col_width;
                        }
                        table_width += (column_widths.len() as f64 + 1.0) * table_border;
                    }

                    table_height += 2.0 * table_border;
                    block.width = table_width;
                    block.height = table_height;
                }
                "thead" | "tbody" | "tfoot" => {
                    let mut section_width: f64 = 0.0;
                    let mut section_height = 0.0;
                    let mut section_column_widths: Vec<Vec<f64>> = Vec::new();

                    for child in node.children() {
                        if child.is_element() && child.tag_name().name() == "tr" {
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            section_height += child_attrs.height;
                            section_width = section_width.max(child_attrs.width);
                            if let ExtraData::Row(widths) = child_attrs.extra_data {
                                section_column_widths.push(widths);
                            }
                        }
                    }

                    block.width = section_width;
                    block.height = section_height;
                    block.extra_data = ExtraData::Section(section_column_widths);
                }
                "td" | "th" => {
                    let cell_font_style = if tag == "th" {
                        FontStyle::Semibold
                    } else {
                        font_style
                    };
                    let mut cell_content_width: f64 = 0.0;
                    let mut cell_content_height = 0.0;

                    for child in node.children() {
                        let child_attrs = measure_node(
                            ruler,
                            depth + 1,
                            child,
                            Some(font_family),
                            mono_font_family,
                            font_size,
                            cell_font_style,
                        );
                        cell_content_width = cell_content_width.max(child_attrs.width);
                        cell_content_height += child_attrs.height;
                    }

                    block.width = cell_content_width;
                    block.height = cell_content_height;
                }
                "tr" => {
                    let mut row_width = 0.0;
                    let mut cell_widths: Vec<f64> = Vec::new();
                    let cell_border = 1.0;
                    let row_border = 1.0;
                    let mut max_cell_height: f64 = 0.0;
                    let mut cell_count = 0usize;

                    let in_header = has_ancestor_element(node, "thead");
                    let row_font_style = if in_header {
                        FontStyle::Semibold
                    } else {
                        font_style
                    };

                    for child in node.children() {
                        if child.is_element() && matches!(child.tag_name().name(), "td" | "th") {
                            cell_count += 1;
                            let child_font_style = if child.tag_name().name() == "th" {
                                FontStyle::Semibold
                            } else {
                                row_font_style
                            };
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                child_font_style,
                            );
                            let cell_width = child_attrs.width + 13.0 * 2.0;
                            let cell_height = child_attrs.height + 6.0 * 2.0;
                            cell_widths.push(cell_width);
                            max_cell_height = max_cell_height.max(cell_height);
                        }
                    }

                    if cell_count > 0 {
                        for width in &cell_widths {
                            row_width += *width;
                        }
                        row_width += (cell_count as f64 + 1.0) * cell_border;
                    }

                    block.width = row_width;
                    block.height = max_cell_height + row_border;
                    block.extra_data = ExtraData::Row(cell_widths);
                }
                _ => {}
            }

            if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                ruler.set_line_height_factor(original_line_height);
            }

            if block.height > 0.0 && block.height < line_height_px {
                block.height = line_height_px;
            }
            block
        }
        _ => BlockAttrs::default(),
    }
}

