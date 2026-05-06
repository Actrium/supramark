pub mod activity;
pub mod board;
pub mod bpm;
pub mod chart;
pub mod chronology;
pub mod component;
pub mod creole_diagram;
pub mod ditaa;
pub mod ebnf;
pub mod erd;
pub mod files_diagram;
pub mod flow;
pub mod gantt;
pub mod git;
pub mod graphviz;
pub mod hcl;
pub mod json_diagram;
pub mod math;
pub mod mindmap;
pub mod nwdiag;
pub mod packet;
pub mod pie;
pub mod regex_diagram;
pub mod salt;
pub mod sequence;
pub mod sequence_teoz;
pub mod state;
pub mod timing;
pub mod usecase;
pub mod wbs;
pub mod wire;

pub use graphviz::{
    layout as layout_graph, layout_with_svek, ClassNoteLayout, EdgeLayout, GraphLayout,
    LayoutClusterSpec, LayoutEdge, LayoutGraph, LayoutNode, NodeLayout, RankDir,
};

use std::collections::HashMap;

use crate::font_metrics;
use crate::model::{
    ArrowHead, ClassDiagram, ClassHideShowRule, ClassPortion, ClassRuleTarget, Diagram, Direction,
    Entity, EntityKind, GroupKind, LineStyle, Member, Stereotype, Visibility,
};
use crate::Result;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct QualifierMargins {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub up_total_width: f64,
    pub down_total_width: f64,
}

/// Unified layout result
#[derive(Debug)]
pub enum DiagramLayout {
    Bpm(bpm::BpmLayout),
    Class(GraphLayout),
    Sequence(sequence::SeqLayout),
    Activity(activity::ActivityLayout),
    State(state::StateLayout),
    Component(component::ComponentLayout),
    Board(board::BoardLayout),
    Chart(chart::ChartLayout),
    Chronology(chronology::ChronologyLayout),
    Ditaa(ditaa::DitaaLayout),
    Erd(erd::ErdLayout),
    Files(files_diagram::FilesLayout),
    Flow(flow::FlowLayout),
    Gantt(gantt::GanttLayout),
    Hcl(hcl::HclLayout),
    Json(json_diagram::JsonLayout),
    Mindmap(mindmap::MindmapLayout),
    Nwdiag(nwdiag::NwdiagLayout),
    Pie(pie::PieLayout),
    Salt(salt::SaltLayout),
    Timing(timing::TimingLayout),
    Wbs(wbs::WbsLayout),
    Yaml(json_diagram::JsonLayout),
    Dot(GraphLayout),
    UseCase(usecase::UseCaseLayout),
    Packet(packet::PacketLayout),
    Git(git::GitLayout),
    Regex(ebnf::EbnfLayout),
    Ebnf(ebnf::EbnfLayout),
    Wire(wire::WireLayout),
    Math(math::MathLayout),
    Latex(math::MathLayout),
    Creole(creole_diagram::CreoleLayout),
    Def(math::MathLayout),
}

// ── Class entity sizing constants — sourced from Java PlantUML ───────
//
// All font metric values from Java AWT FontMetrics at full f64 precision.
// See tests/tools/ExtractFontMetrics.java for extraction method.

/// FontParam.CLASS = 12pt but EntityImageClassHeader renders name at 14pt.
const CLASS_FONT_SIZE: f64 = 14.0;
/// FontParam.CLASS_ATTRIBUTE = 10pt.
#[allow(dead_code)] // Java-ported constant for class attribute sizing
const CLASS_ATTR_FONT_SIZE: f64 = 10.0;
/// MethodsOrFieldsArea: empty compartment = margin_top(4) + margin_bottom(4).
const LINE_HEIGHT_PT: f64 = 8.0;
/// EntityImageClassHeader.java:150 — withMargin(circledChar, left=4, ...).
const CIRCLE_LEFT_PAD: f64 = 4.0;
/// SkinParam.circledCharacterRadius = 17/3+6 = 11. Diameter = 2 * 11 = 22.
const CIRCLE_DIAMETER: f64 = 22.0;
/// Gap between circle block right edge and name text left edge.
/// HeaderLayout: name block starts right after circle block (no explicit gap).
/// But EntityImageClassHeader name margin left=3, and circleBlock right margin=0.
/// So effective gap = name_margin_left(3). This 3 is the same as RIGHT_PAD.
const CIRCLE_TEXT_GAP: f64 = 3.0;
/// EntityImageClassHeader.java:105 — withMargin(name, 3, 3, 0, 0): right=3.
const RIGHT_PAD: f64 = 3.0;
/// HeaderLayout height = max(circleDim.h=32, ...) = 32.
const HEADER_HEIGHT_PT: f64 = 32.0;
/// MethodsOrFieldsArea: empty section = margin_top(4) + margin_bottom(4) = 8.
const EMPTY_COMPARTMENT: f64 = 8.0;
/// CircledChar block: diameter(22) + marginLeft(4) + marginRight(0) = 26.
const HEADER_CIRCLE_BLOCK_WIDTH: f64 = 26.0;
/// CircledChar block: diameter(22) + marginTop(5) + marginBottom(5) = 32.
const HEADER_CIRCLE_BLOCK_HEIGHT: f64 = 32.0;
/// SansSerif 14pt: ascent(12.995117) + descent(3.301758) = 16.296875.
const HEADER_NAME_BLOCK_HEIGHT: f64 = 16.296875;
/// Name margin: withMargin(name, 3, 3, 0, 0) → left(3) + right(3) = 6.
const HEADER_NAME_BLOCK_MARGIN_X: f64 = 6.0;
// Stereo margin: withMargin(stereo, 1, 0) → TextBlockMarged(0, 1, 0, 1) → left(1) + right(1) = 2.
const HEADER_STEREO_BLOCK_MARGIN_X: f64 = 2.0;
/// FontParam.CLASS_STEREOTYPE = 12pt.
const HEADER_STEREO_FONT_SIZE: f64 = 12.0;
/// SansSerif 12pt italic: ascent(11.138672) + descent(2.830078) = 13.96875.
const HEADER_STEREO_LINE_HEIGHT: f64 = 13.96875;
/// HeaderLayout.java:77 — height includes stereoDim.h + nameDim.h + 10 (gap).
const HEADER_STEREO_NAME_GAP: f64 = 10.0;
/// SansSerif 14pt height = 16.296875 (used for member row layout).
const MEMBER_ROW_HEIGHT: f64 = 16.296875;
/// margin_top(4) + MEMBER_ROW_HEIGHT(16.296875) + margin_bottom(4) = 24.296875.
#[allow(dead_code)] // Java-ported constant for class member sizing
const MEMBER_BLOCK_HEIGHT_ONE_ROW: f64 = 24.296875;
const MEMBER_TEXT_LEFT_WITH_ICON: f64 = 26.0;
const MEMBER_TEXT_LEFT_NO_ICON: f64 = 6.0;
/// VisibilityModifier.getUBlock: (size+1, size+1) where size = circledCharacterRadius - 1 = 10.
/// Block width = 11. Used when entity has a visibility modifier (e.g. -class foo).
const ENTITY_VIS_ICON_BLOCK_WIDTH: f64 = 11.0;

// -- Generic type box constants -- sourced from EntityImageClassHeader.java --
//
// EntityImageClassHeader.java:136-145: generic block =
//   text(12pt italic) + innerMargin(1,1) + TextBlockGeneric(dashed rect) + outerMargin(1,1)
// HeaderLayout.java:112: delta=4, xGeneric=width-genericDim.w+4, yGeneric=-4

/// Generic type text font size (FontParam.CLASS_STEREOTYPE = 12pt italic).
const GENERIC_FONT_SIZE: f64 = 12.0;
/// Inner margin around generic text (withMargin(genericBlock, 1, 1), line 139).
const GENERIC_INNER_MARGIN: f64 = 1.0;
/// Outer margin around TextBlockGeneric (withMargin(genericBlock, 1, 1), line 145).
const GENERIC_OUTER_MARGIN: f64 = 1.0;
/// Group header title/stereotype font size.
const GROUP_HEADER_FONT_SIZE: f64 = 14.0;

// ── Object entity sizing constants — sourced from EntityImageObject.java ──
//
// EntityImageObject.java:98 — withMargin(tmp, 2, 2) → margin(top=2, right=2, bottom=2, left=2).
// EntityImageObject.java:228 — xMarginCircle = 5.
// EntityImageObject.java:110-112 — empty fields = TextBlockLineBefore(lineThickness,
//   TextBlockEmpty(10, 16)) → dim = (10, 16).

/// EntityImageObject.java:98 — name block margin (all sides).
const OBJ_NAME_MARGIN: f64 = 2.0;
/// EntityImageObject.java:228 — xMarginCircle = 5.
const OBJ_X_MARGIN_CIRCLE: f64 = 5.0;
/// EntityImageObject.java:112 — TextBlockEmpty(10, 16).height = 16.
const OBJ_EMPTY_BODY_HEIGHT: f64 = 16.0;
/// EntityImageObject.java:112 — TextBlockEmpty(10, 16).width = 10.
const OBJ_EMPTY_BODY_WIDTH: f64 = 10.0;

/// Perform layout on a Diagram
pub fn layout(diagram: &Diagram, skin: &crate::style::SkinParams) -> Result<DiagramLayout> {
    match diagram {
        Diagram::Bpm(bd) => {
            let bl = bpm::layout_bpm(bd)?;
            Ok(DiagramLayout::Bpm(bl))
        }
        Diagram::Class(cd) => {
            let gl = layout_class_diagram(cd, skin)?;
            Ok(DiagramLayout::Class(gl))
        }
        Diagram::Sequence(sd) => {
            let sl = if sd.teoz_mode {
                sequence_teoz::layout_sequence_teoz(sd, skin)?
            } else {
                sequence::layout_sequence(sd, skin)?
            };
            Ok(DiagramLayout::Sequence(sl))
        }
        Diagram::Activity(ad) => {
            let al = activity::layout_activity(ad)?;
            Ok(DiagramLayout::Activity(al))
        }
        Diagram::State(sd) => {
            let sl = state::layout_state(sd)?;
            Ok(DiagramLayout::State(sl))
        }
        Diagram::Component(cd) => {
            let cl = component::layout_component(cd, skin)?;
            Ok(DiagramLayout::Component(cl))
        }
        Diagram::Chart(cd) => {
            let cl = chart::layout_chart(cd)?;
            Ok(DiagramLayout::Chart(cl))
        }
        Diagram::Ditaa(dd) => {
            let dl = ditaa::layout_ditaa(dd)?;
            Ok(DiagramLayout::Ditaa(dl))
        }
        Diagram::Erd(ed) => {
            let el = erd::layout_erd(ed)?;
            Ok(DiagramLayout::Erd(el))
        }
        Diagram::Files(fd) => {
            let fl = files_diagram::layout_files(fd)?;
            Ok(DiagramLayout::Files(fl))
        }
        Diagram::Flow(fd) => {
            let fl = flow::layout_flow(fd)?;
            Ok(DiagramLayout::Flow(fl))
        }
        Diagram::Gantt(gd) => {
            let gl = gantt::layout_gantt(gd)?;
            Ok(DiagramLayout::Gantt(gl))
        }
        Diagram::Json(jd) => {
            let jl = json_diagram::layout_json(jd)?;
            Ok(DiagramLayout::Json(jl))
        }
        Diagram::Mindmap(md) => {
            let ml = mindmap::layout_mindmap(md, skin)?;
            Ok(DiagramLayout::Mindmap(ml))
        }
        Diagram::Nwdiag(nd) => {
            let nl = nwdiag::layout_nwdiag(nd)?;
            Ok(DiagramLayout::Nwdiag(nl))
        }
        Diagram::Salt(sd) => {
            let sl = salt::layout_salt(sd)?;
            Ok(DiagramLayout::Salt(sl))
        }
        Diagram::Timing(td) => {
            let tl = timing::layout_timing(td, skin)?;
            Ok(DiagramLayout::Timing(tl))
        }
        Diagram::Wbs(wd) => {
            let wl = wbs::layout_wbs(wd)?;
            Ok(DiagramLayout::Wbs(wl))
        }
        Diagram::Yaml(yd) => {
            let yl = json_diagram::layout_json(yd)?;
            Ok(DiagramLayout::Yaml(yl))
        }
        Diagram::UseCase(ud) => {
            // Route through the component (description diagram) layout pipeline,
            // same as Java's CucaDiagramFileMakerSvek for usecase diagrams.
            let cd = crate::model::component::ComponentDiagram::from(ud);
            let cl = component::layout_component(&cd, skin)?;
            Ok(DiagramLayout::Component(cl))
        }
        Diagram::Dot(dd) => {
            // DOT passthrough: use a minimal placeholder layout
            let lg = LayoutGraph {
                nodes: vec![LayoutNode {
                    id: "dot_root".into(),
                    label: "DOT".into(),
                    width_pt: 200.0,
                    height_pt: 100.0,
                    shape: None,
                    shield: None,
                    entity_position: None,
                    max_label_width: None,
                    port_label_width: None,
                    order: None,
                    image_width_pt: None,
                    image_height_pt: None,
                    lf_extra_left: 0.0,
                    lf_rect_correction: true,
                    lf_has_body_separator: false,
                    lf_node_polygon: false,
                    lf_polygon_hack: false,
                    lf_actor_stickman: false,
                    hidden: false,
                }],
                edges: vec![],
                clusters: vec![],
                rankdir: RankDir::TopToBottom,
                is_activity: false,
                ranksep_override: None,
                nodesep_override: None,
                use_simplier_dot_link_strategy: false,
                arrow_font_size: None,
            };
            let gl = graphviz::layout(&lg)?;
            let _ = &dd.source;
            Ok(DiagramLayout::Dot(gl))
        }
        Diagram::Packet(pd) => {
            let pl = packet::layout_packet(pd)?;
            Ok(DiagramLayout::Packet(pl))
        }
        Diagram::Git(gd) => {
            let gl = git::layout_git(gd)?;
            Ok(DiagramLayout::Git(gl))
        }
        Diagram::Regex(rd) => {
            let expr = crate::parser::regex_diagram::regex_node_to_ebnf(&rd.node);
            let rl = ebnf::layout_regex_as_ebnf(&expr)?;
            Ok(DiagramLayout::Regex(rl))
        }
        Diagram::Ebnf(ed) => {
            let el = ebnf::layout_ebnf(ed)?;
            Ok(DiagramLayout::Ebnf(el))
        }
        Diagram::Pie(pd) => {
            let pl = pie::layout_pie(pd)?;
            Ok(DiagramLayout::Pie(pl))
        }
        Diagram::Board(bd) => {
            let bl = board::layout_board(bd)?;
            Ok(DiagramLayout::Board(bl))
        }
        Diagram::Chronology(cd) => {
            let cl = chronology::layout_chronology(cd)?;
            Ok(DiagramLayout::Chronology(cl))
        }
        Diagram::Hcl(hd) => {
            let hl = hcl::layout_hcl(hd)?;
            Ok(DiagramLayout::Hcl(hl))
        }
        Diagram::Wire(wd) => {
            let wl = wire::layout_wire(wd)?;
            Ok(DiagramLayout::Wire(wl))
        }
        Diagram::Math(md) => {
            let ml = math::layout_math(md)?;
            Ok(DiagramLayout::Math(ml))
        }
        Diagram::Latex(ld) => {
            let ll = math::layout_math(ld)?;
            Ok(DiagramLayout::Latex(ll))
        }
        Diagram::Creole(cd) => {
            let cl = creole_diagram::layout_creole(cd)?;
            Ok(DiagramLayout::Creole(cl))
        }
        Diagram::Def(dd) => {
            let dl = math::layout_def(dd)?;
            Ok(DiagramLayout::Def(dl))
        }
    }
}

/// Replace DOT-incompatible characters with safe identifiers
fn sanitize_id(name: &str) -> String {
    name.replace('<', "_LT_")
        .replace('>', "_GT_")
        .replace(',', "_COMMA_")
        .replace('.', "_DOT_")
        .replace(' ', "_")
}

fn cluster_id(name: &str) -> String {
    format!("grp_{}", sanitize_id(name))
}

pub(crate) fn compute_entity_qualifier_margins(
    cd: &ClassDiagram,
) -> HashMap<String, QualifierMargins> {
    let mut margins: HashMap<String, QualifierMargins> = HashMap::new();

    for link in &cd.links {
        if let Some(text) = &link.from_qualifier {
            let dim = qualifier_box_dimension(text);
            let entry = margins.entry(link.from.clone()).or_default();
            if link.arrow_len == 1 {
                entry.right = entry.right.max(dim.0);
            } else {
                entry.bottom = entry.bottom.max(dim.1);
                entry.down_total_width += dim.0;
            }
        }
        if let Some(text) = &link.to_qualifier {
            let dim = qualifier_box_dimension(text);
            let entry = margins.entry(link.to.clone()).or_default();
            if link.arrow_len == 1 {
                entry.left = entry.left.max(dim.0);
            } else {
                entry.top = entry.top.max(dim.1);
                entry.up_total_width += dim.0;
            }
        }
    }

    margins
}

fn qualifier_box_dimension(text: &str) -> (f64, f64) {
    let text_w = font_metrics::text_width(text, "SansSerif", 14.0, false, false);
    let text_h = font_metrics::line_height("SansSerif", 14.0, false, false);
    (text_w + 4.0, text_h + 2.0)
}

fn arrow_head_to_svek_decoration(head: &ArrowHead) -> crate::svek::edge::LinkDecoration {
    match head {
        ArrowHead::None => crate::svek::edge::LinkDecoration::None,
        ArrowHead::Arrow => crate::svek::edge::LinkDecoration::Arrow,
        ArrowHead::Triangle => crate::svek::edge::LinkDecoration::Extends,
        ArrowHead::Diamond => crate::svek::edge::LinkDecoration::Composition,
        ArrowHead::DiamondHollow => crate::svek::edge::LinkDecoration::Aggregation,
        ArrowHead::Plus => crate::svek::edge::LinkDecoration::Plus,
    }
}

fn link_style_to_svek(line_style: &LineStyle) -> crate::svek::edge::LinkStyle {
    match line_style {
        LineStyle::Solid => crate::svek::edge::LinkStyle::Normal,
        LineStyle::Dashed => crate::svek::edge::LinkStyle::Dashed,
    }
}

/// Compute generic block outer dimension width (genericDim.width in Java).
/// Returns 0 when entity has no generic parameter.
fn generic_dim_width(entity: &Entity) -> f64 {
    match entity.generic {
        Some(ref g) => {
            let text_w = font_metrics::text_width(g, "SansSerif", GENERIC_FONT_SIZE, false, true);
            text_w + 2.0 * GENERIC_INNER_MARGIN + 2.0 * GENERIC_OUTER_MARGIN
        }
        None => 0.0,
    }
}

/// Split an entity name into display lines following Java Display semantics.
/// Java splits on `\n` (literal backslash-n), `\r` (literal backslash-r), and NEWLINE_CHAR.
/// Empty lines are preserved (rendered as non-breaking space).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplayLine {
    pub text: String,
    pub leading_tabs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplayBlock {
    pub alignment: DisplayAlignment,
    pub lines: Vec<DisplayLine>,
}

pub(crate) fn display_tab_width(font_size: f64, bold: bool, italic: bool) -> f64 {
    8.0 * font_metrics::text_width(" ", "SansSerif", font_size, bold, italic)
}

pub(crate) fn display_line_metrics(
    line: &DisplayLine,
    font_size: f64,
    bold: bool,
    italic: bool,
) -> (f64, f64) {
    let indent_width = line.leading_tabs as f64 * display_tab_width(font_size, bold, italic);
    let visible = if line.text.is_empty() {
        "\u{00A0}".to_string()
    } else {
        line.text.clone()
    };
    let visible_width = font_metrics::text_width(&visible, "SansSerif", font_size, bold, italic);
    (visible_width, indent_width)
}

/// Result of stripping HTML-like markup tags from a display name.
///
/// Java PlantUML interprets `<b>`, `<i>`, `<u>`, `<s>` etc. in display names
/// as formatting directives. The tags are stripped and the formatting is applied
/// to the rendered text.
#[derive(Debug, Clone)]
pub(crate) struct StrippedMarkup {
    /// The plain text with all markup tags removed.
    pub text: String,
    /// Whether `<b>` was found → font-weight: bold
    pub bold: bool,
    /// Whether `<i>` was found → font-style: italic
    pub italic: bool,
    /// Whether `<u>` was found → text-decoration: underline
    #[allow(dead_code)] // parsed for future text-decoration rendering
    pub underline: bool,
    /// Whether `<s>` or `<strike>` was found → text-decoration: line-through
    #[allow(dead_code)] // parsed for future text-decoration rendering
    pub strikethrough: bool,
}

/// Strip HTML-like markup tags from a display name string.
///
/// Recognizes `<b>`, `</b>`, `<i>`, `</i>`, `<u>`, `</u>`, `<s>`, `</s>`,
/// `<strike>`, `</strike>`. Everything else is left as-is.
pub(crate) fn strip_html_markup(text: &str) -> StrippedMarkup {
    let mut result = String::with_capacity(text.len());
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut strikethrough = false;
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Try to match a known tag
            if let Some((tag_len, is_open, tag_kind)) = match_html_tag(&text[i..]) {
                match tag_kind {
                    HtmlTag::Bold => bold |= is_open,
                    HtmlTag::Italic => italic |= is_open,
                    HtmlTag::Underline => underline |= is_open,
                    HtmlTag::Strike => strikethrough |= is_open,
                }
                i += tag_len;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    StrippedMarkup {
        text: result,
        bold,
        italic,
        underline,
        strikethrough,
    }
}

enum HtmlTag {
    Bold,
    Italic,
    Underline,
    Strike,
}

/// Try to match a known HTML tag at the start of `s`.
/// Returns (byte length consumed, is_opening_tag, tag_kind).
fn match_html_tag(s: &str) -> Option<(usize, bool, HtmlTag)> {
    let lower = s.to_ascii_lowercase();
    let tags: &[(&str, bool, HtmlTag)] = &[
        ("<b>", true, HtmlTag::Bold),
        ("</b>", false, HtmlTag::Bold),
        ("<i>", true, HtmlTag::Italic),
        ("</i>", false, HtmlTag::Italic),
        ("<u>", true, HtmlTag::Underline),
        ("</u>", false, HtmlTag::Underline),
        ("<s>", true, HtmlTag::Strike),
        ("</s>", false, HtmlTag::Strike),
        ("<strike>", true, HtmlTag::Strike),
        ("</strike>", false, HtmlTag::Strike),
    ];
    for (tag, is_open, kind) in tags {
        if lower.starts_with(tag) {
            return Some((
                tag.len(),
                *is_open,
                match kind {
                    HtmlTag::Bold => HtmlTag::Bold,
                    HtmlTag::Italic => HtmlTag::Italic,
                    HtmlTag::Underline => HtmlTag::Underline,
                    HtmlTag::Strike => HtmlTag::Strike,
                },
            ));
        }
    }
    None
}

/// Convert a qualified entity name to the display name Java uses in class boxes.
///
/// For `pkg1.pkg2.Class`, the rendered name is `Class` while the qualified name
/// remains `pkg1.pkg2.Class` for IDs, links, and interactive metadata.
pub(crate) fn class_entity_display_name(name: &str) -> String {
    let mut first_break = name.len();
    for needle in ["\\r", "\\n"] {
        if let Some(pos) = name.find(needle) {
            first_break = first_break.min(pos);
        }
    }
    if let Some(pos) = name.find(crate::NEWLINE_CHAR) {
        first_break = first_break.min(pos);
    }
    if let Some(pos) = name.find('\r') {
        first_break = first_break.min(pos);
    }
    if let Some(pos) = name.find('\n') {
        first_break = first_break.min(pos);
    }
    let (head, tail) = name.split_at(first_break);
    let short_head = head.rsplit('.').next().unwrap_or(head);
    format!("{short_head}{tail}")
}

fn group_display_name(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct ClassGroupHeaderMetrics {
    pub visible_stereotypes: Vec<String>,
    pub label_width: f64,
    pub label_height: f64,
}

pub(crate) fn class_group_header_metrics(
    group: &crate::model::Group,
    rules: &[ClassHideShowRule],
) -> ClassGroupHeaderMetrics {
    let title = group_display_name(&group.name);
    let title_width =
        font_metrics::text_width(&title, "SansSerif", GROUP_HEADER_FONT_SIZE, true, false);
    let title_height = font_metrics::line_height("SansSerif", GROUP_HEADER_FONT_SIZE, true, false);
    let stereo_line_height =
        font_metrics::line_height("SansSerif", GROUP_HEADER_FONT_SIZE, false, true);
    let visible_stereotypes = visible_stereotype_labels(rules, &group.stereotypes);
    let stereo_width = visible_stereotypes
        .iter()
        .map(|label| {
            font_metrics::text_width(
                &format!("\u{00AB}{label}\u{00BB}"),
                "SansSerif",
                GROUP_HEADER_FONT_SIZE,
                false,
                true,
            )
        })
        .fold(0.0_f64, f64::max);
    let stereo_height = visible_stereotypes.len() as f64 * stereo_line_height;

    ClassGroupHeaderMetrics {
        visible_stereotypes,
        label_width: title_width.max(stereo_width).floor().max(0.0),
        label_height: (title_height + stereo_height).floor().max(0.0),
    }
}

fn build_layout_clusters(
    cd: &ClassDiagram,
    name_to_id: &HashMap<String, String>,
) -> Vec<LayoutClusterSpec> {
    use crate::model::Group;
    use crate::svek::cluster::ClusterStyle;

    fn parent_group_name(group_name: &str, all_names: &[String]) -> Option<String> {
        all_names
            .iter()
            .filter(|candidate| candidate.as_str() != group_name)
            .filter(|candidate| group_name.starts_with(candidate.as_str()))
            .filter(|candidate| group_name.as_bytes().get(candidate.len()) == Some(&b'.'))
            .max_by_key(|candidate| candidate.len())
            .cloned()
    }

    fn build_cluster_recursive(
        group: &Group,
        cd: &ClassDiagram,
        children_by_parent: &HashMap<Option<String>, Vec<&Group>>,
        name_to_id: &HashMap<String, String>,
    ) -> LayoutClusterSpec {
        let header_metrics = class_group_header_metrics(group, &cd.hide_show_rules);
        let mut children = children_by_parent
            .get(&Some(group.name.clone()))
            .cloned()
            .unwrap_or_default();
        children.sort_by_key(|child| {
            (
                child.source_line.unwrap_or(usize::MAX),
                child.name.matches('.').count(),
                child.name.clone(),
            )
        });
        LayoutClusterSpec {
            id: cluster_id(&group.name),
            qualified_name: group.name.clone(),
            title: Some(group_display_name(&group.name)),
            style: match group.kind {
                GroupKind::Rectangle => ClusterStyle::Rectangle,
                GroupKind::Package | GroupKind::Namespace => ClusterStyle::Package,
            },
            label_width: Some(header_metrics.label_width),
            label_height: Some(header_metrics.label_height),
            node_ids: group
                .entities
                .iter()
                .map(|name| {
                    name_to_id
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| sanitize_id(name))
                })
                .collect(),
            sub_clusters: children
                .into_iter()
                .map(|child| build_cluster_recursive(child, cd, children_by_parent, name_to_id))
                .collect(),
            order: group.source_line,
            has_link_from_or_to_group: false,
            special_point_id: None,
        }
    }

    let group_names: Vec<String> = cd.groups.iter().map(|group| group.name.clone()).collect();
    let mut children_by_parent: HashMap<Option<String>, Vec<&crate::model::Group>> = HashMap::new();
    for group in &cd.groups {
        let parent = parent_group_name(&group.name, &group_names);
        children_by_parent.entry(parent).or_default().push(group);
    }
    let mut roots = children_by_parent.remove(&None).unwrap_or_default();
    roots.sort_by_key(|group| {
        (
            group.source_line.unwrap_or(usize::MAX),
            group.name.matches('.').count(),
            group.name.clone(),
        )
    });
    roots
        .into_iter()
        .map(|group| build_cluster_recursive(group, cd, &children_by_parent, name_to_id))
        .collect()
}

pub(crate) fn split_name_display(name: &str) -> DisplayBlock {
    let mut lines = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = name.chars().collect();
    let mut alignment = DisplayAlignment::Center;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'r' => {
                    lines.push(current);
                    current = String::new();
                    alignment = DisplayAlignment::Right;
                    i += 2;
                    continue;
                }
                'l' => {
                    lines.push(current);
                    current = String::new();
                    alignment = DisplayAlignment::Left;
                    i += 2;
                    continue;
                }
                'n' => {
                    lines.push(current);
                    current = String::new();
                    i += 2;
                    continue;
                }
                't' => {
                    current.push('\t');
                    i += 2;
                    continue;
                }
                '\\' => {
                    current.push('\\');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        if matches!(chars[i], '\r' | '\n') || chars[i] == crate::NEWLINE_CHAR {
            lines.push(current);
            current = String::new();
            i += 1;
            continue;
        }
        current.push(chars[i]);
        i += 1;
    }
    lines.push(current);

    let lines = lines
        .into_iter()
        .map(|line| {
            let leading_tabs = line.chars().take_while(|ch| *ch == '\t').count();
            let text = line.chars().skip(leading_tabs).collect::<String>();
            DisplayLine { text, leading_tabs }
        })
        .collect();

    DisplayBlock { alignment, lines }
}

#[allow(dead_code)] // utility for future name splitting
pub(crate) fn split_name_display_lines(name: &str) -> Vec<String> {
    let block = split_name_display(name);
    let mut lines = Vec::new();
    for line in block.lines {
        lines.push(line.text);
    }
    if lines.is_empty() {
        vec![name.to_string()]
    } else {
        lines
    }
}

/// Estimate entity rendering size (width_pt, height_pt)
fn estimate_entity_size(
    cd: &ClassDiagram,
    entity: &Entity,
    member_row_h: f64,
    name_font_size: f64,
    attr_font_size: f64,
) -> (f64, f64) {
    if matches!(entity.kind, EntityKind::Enum | EntityKind::Annotation) {
        return estimate_entity_size_legacy(entity);
    }

    if matches!(entity.kind, EntityKind::Object | EntityKind::Map) {
        return estimate_object_size(entity, attr_font_size);
    }

    if entity.kind == EntityKind::Rectangle && !entity.description.is_empty() {
        return estimate_rectangle_size(entity);
    }

    if entity.kind == EntityKind::Component {
        return estimate_component_size(cd, entity, name_font_size);
    }

    // Entity name WITHOUT generic parameter -- generic is rendered separately.
    // Split name into display lines following Java Display semantics (\n, \r split).
    // When `as Alias` is used, display_name holds the original quoted label.
    let name_display_raw = entity
        .display_name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| class_entity_display_name(&entity.name));
    // Strip HTML markup (<b>, <i>, etc.) so width is calculated on plain text
    let markup = strip_html_markup(&name_display_raw);
    let name_display = if markup.bold || markup.italic {
        markup.text.clone()
    } else {
        name_display_raw
    };
    let name_bold = markup.bold;
    let name_block = split_name_display(&name_display);
    let n_name_lines = name_block.lines.len();

    let visible_stereotypes = visible_stereotype_labels(&cd.hide_show_rules, &entity.stereotypes);
    let italic_name =
        markup.italic || matches!(entity.kind, EntityKind::Abstract | EntityKind::Interface);
    let name_width = name_block
        .lines
        .iter()
        .map(|line| {
            let (visible_width, indent_width) =
                display_line_metrics(line, name_font_size, name_bold, italic_name);
            visible_width + indent_width
        })
        .fold(0.0_f64, f64::max);
    let name_block_width = name_width + HEADER_NAME_BLOCK_MARGIN_X;
    let name_block_height = n_name_lines as f64 * HEADER_NAME_BLOCK_HEIGHT;
    let stereo_block_width = visible_stereotypes
        .iter()
        .map(|label| {
            let stereo_text = format!("\u{00AB}{label}\u{00BB}");
            font_metrics::text_width(
                &stereo_text,
                "SansSerif",
                HEADER_STEREO_FONT_SIZE,
                false,
                true,
            ) + HEADER_STEREO_BLOCK_MARGIN_X
        })
        .fold(0.0_f64, f64::max);
    let vis_icon_w = if entity.visibility.is_some() {
        ENTITY_VIS_ICON_BLOCK_WIDTH
    } else {
        0.0
    };
    // HeaderLayout.java:74 -- width = circleDim.w + max(stereoDim.w, nameDim.w) + genericDim.w
    let gen_w = generic_dim_width(entity);
    let header_width =
        HEADER_CIRCLE_BLOCK_WIDTH + vis_icon_w + name_block_width.max(stereo_block_width) + gen_w;
    let stereo_height = visible_stereotypes.len() as f64 * HEADER_STEREO_LINE_HEIGHT;
    let header_height =
        HEADER_CIRCLE_BLOCK_HEIGHT.max(stereo_height + name_block_height + HEADER_STEREO_NAME_GAP);

    let raw_field_count = entity.members.iter().filter(|m| !m.is_method).count();
    let raw_method_count = entity.members.iter().filter(|m| m.is_method).count();

    let visible_fields: Vec<&Member> = entity
        .members
        .iter()
        .filter(|m| !m.is_method)
        .filter(|_| {
            show_portion(
                &cd.hide_show_rules,
                ClassPortion::Field,
                &entity.name,
                raw_field_count,
            )
        })
        .collect();
    let visible_methods: Vec<&Member> = entity
        .members
        .iter()
        .filter(|m| m.is_method)
        .filter(|_| {
            show_portion(
                &cd.hide_show_rules,
                ClassPortion::Method,
                &entity.name,
                raw_method_count,
            )
        })
        .collect();

    let show_fields = show_portion(
        &cd.hide_show_rules,
        ClassPortion::Field,
        &entity.name,
        raw_field_count,
    );
    let show_methods = show_portion(
        &cd.hide_show_rules,
        ClassPortion::Method,
        &entity.name,
        raw_method_count,
    );

    let body_width = estimate_members_width(&visible_fields, attr_font_size)
        .max(estimate_members_width(&visible_methods, attr_font_size));
    let body_height = section_height(show_fields, &visible_fields, member_row_h)
        + section_height(show_methods, &visible_methods, member_row_h);

    let width = header_width.max(body_width);
    let height = header_height + body_height;

    log::debug!(
        "estimate_entity_size: {} -> ({}, {})",
        entity.name,
        width,
        height
    );

    (width, height)
}

/// Estimate size for Object entities (EntityImageObject.java layout).
///
/// Object header: name with margin(2,2,2,2) centered, no circle icon.
/// Body: TextBlockLineBefore(lineThickness, TextBlockEmpty(10, 16)) for empty fields.
/// Width = max(bodyWidth, titleWidth + 2 * xMarginCircle).
/// Height = titleHeight + bodyHeight.
/// Estimate size for a rectangle entity with bracket-body description.
/// Java: body text at font-size 14, padding 10px, no header/separator.
fn estimate_rectangle_size(entity: &Entity) -> (f64, f64) {
    let desc_font_size = 14.0_f64;
    let padding = 10.0;

    // Use creole measurement to account for table syntax, inline markup, etc.
    // preserve_backslash_n=true: Java keeps literal \n as displayable text in bracket bodies.
    let (content_w, content_h) = crate::render::svg_richtext::measure_creole_display_lines(
        &entity.description,
        "SansSerif",
        desc_font_size,
        false,
        false,
        true,
    );

    let width = content_w + 2.0 * padding;
    let height = content_h + 2.0 * padding;

    log::debug!(
        "estimate_rectangle_size: {} -> ({:.2}, {:.2}) [{} desc lines, content {:.2}x{:.2}]",
        entity.name,
        width,
        height,
        entity.description.len(),
        content_w,
        content_h,
    );
    (width, height)
}

/// Estimate size for `component` entities in the class pipeline.
///
/// Mirrors Java `USymbolComponent2.asSmall.calculateDimension`:
///   margin = (x1=15, x2=25, y1=20, y2=10)
///   dim    = mergeTB(stereotype, label) + margin
///
/// The label is the entity display name rendered at the class font size
/// (default 14pt). When stereotypes are present, their height stacks above
/// the name and the wider of the two drives the inner width.
fn estimate_component_size(cd: &ClassDiagram, entity: &Entity, name_font_size: f64) -> (f64, f64) {
    // Java margin for USymbolComponent2: Margin(10+5, 20+5, 15+5, 5+5)
    // = (x1 left=15, x2 right=25, y1 top=20, y2 bottom=10)
    const MARGIN_LEFT: f64 = 15.0;
    const MARGIN_RIGHT: f64 = 25.0;
    const MARGIN_TOP: f64 = 20.0;
    const MARGIN_BOTTOM: f64 = 10.0;

    let name_display_raw = entity
        .display_name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| class_entity_display_name(&entity.name));
    let markup = strip_html_markup(&name_display_raw);
    let name_display = if markup.bold || markup.italic {
        markup.text.clone()
    } else {
        name_display_raw
    };
    let name_bold = markup.bold;
    let name_italic = markup.italic;
    let name_block = split_name_display(&name_display);
    let n_name_lines = name_block.lines.len().max(1);
    let label_width = name_block
        .lines
        .iter()
        .map(|line| {
            let (visible_width, indent_width) =
                display_line_metrics(line, name_font_size, name_bold, name_italic);
            visible_width + indent_width
        })
        .fold(0.0_f64, f64::max);
    let line_h = font_metrics::line_height("SansSerif", name_font_size, name_bold, name_italic);
    let label_height = n_name_lines as f64 * line_h;

    let visible_stereotypes = visible_stereotype_labels(&cd.hide_show_rules, &entity.stereotypes);
    let stereo_width = visible_stereotypes
        .iter()
        .map(|label| {
            let stereo_text = format!("\u{00AB}{label}\u{00BB}");
            font_metrics::text_width(
                &stereo_text,
                "SansSerif",
                HEADER_STEREO_FONT_SIZE,
                false,
                true,
            )
        })
        .fold(0.0_f64, f64::max);
    let stereo_line_h =
        font_metrics::line_height("SansSerif", HEADER_STEREO_FONT_SIZE, false, true);
    let stereo_height = visible_stereotypes.len() as f64 * stereo_line_h;

    let inner_w = label_width.max(stereo_width);
    let inner_h = label_height + stereo_height;

    let width = inner_w + MARGIN_LEFT + MARGIN_RIGHT;
    let height = inner_h + MARGIN_TOP + MARGIN_BOTTOM;

    log::debug!(
        "estimate_component_size: {} -> ({:.4}, {:.4}) [label {:.4}x{:.4}, stereo {:.4}x{:.4}]",
        entity.name,
        width,
        height,
        label_width,
        label_height,
        stereo_width,
        stereo_height,
    );
    (width, height)
}

fn estimate_object_size(entity: &Entity, attr_font_size: f64) -> (f64, f64) {
    let nd = entity.display_name.as_deref().unwrap_or(&entity.name);
    let nw = if nd.contains("**") || nd.contains("//") {
        crate::render::svg_richtext::measure_creole_display_lines(
            &[nd.to_string()],
            "SansSerif",
            CLASS_FONT_SIZE,
            false,
            false,
            false,
        )
        .0
    } else {
        font_metrics::text_width(nd, "SansSerif", CLASS_FONT_SIZE, false, false)
    };
    let name_block_width = nw + 2.0 * OBJ_NAME_MARGIN;
    let name_block_height = HEADER_NAME_BLOCK_HEIGHT + 2.0 * OBJ_NAME_MARGIN;
    let title_width = name_block_width;
    let title_height = name_block_height;
    let vf: Vec<&Member> = entity.members.iter().filter(|m| !m.is_method).collect();
    let (body_width, body_height) = if entity.kind == EntityKind::Map
        && !entity.map_entries.is_empty()
    {
        // Java TextBlockMap: withMargin(result, 5, 2) → 5px left + 5px right = 10px per column
        let mx = 10.0;
        let (mut ca, mut cb): (f64, f64) = (0.0, 0.0);
        // Java EntityImageMap: each row is wrapped in withMargin(text, 2, 2)
        // adding 4px vertical margin per row.
        let rh = font_metrics::line_height("SansSerif", attr_font_size, false, false) + 4.0;
        for (k, v) in &entity.map_entries {
            ca =
                ca.max(font_metrics::text_width(k, "SansSerif", attr_font_size, false, false) + mx);
            cb =
                cb.max(font_metrics::text_width(v, "SansSerif", attr_font_size, false, false) + mx);
        }
        (ca + cb, entity.map_entries.len() as f64 * rh)
    } else if !vf.is_empty() {
        (
            estimate_members_width(&vf, attr_font_size) + 6.0,
            section_height(true, &vf, MEMBER_ROW_HEIGHT),
        )
    } else {
        (OBJ_EMPTY_BODY_WIDTH, OBJ_EMPTY_BODY_HEIGHT)
    };
    let width = body_width.max(title_width + 2.0 * OBJ_X_MARGIN_CIRCLE);
    let height = title_height + body_height;
    log::debug!(
        "estimate_object_size: {} -> ({:.2}, {:.2})",
        entity.name,
        width,
        height
    );
    (width, height)
}

fn estimate_entity_size_legacy(entity: &Entity) -> (f64, f64) {
    // Entity name WITHOUT generic parameter -- generic is rendered separately
    // When `as Alias` is used, display_name holds the original quoted label.
    let name_display = entity
        .display_name
        .as_deref()
        .map(String::from)
        .unwrap_or_else(|| class_entity_display_name(&entity.name));

    // check if a stereotype line is needed (interface / enum / abstract / custom stereotype)
    let has_stereotype_line = !entity.stereotypes.is_empty()
        || matches!(
            entity.kind,
            EntityKind::Interface | EntityKind::Enum | EntityKind::Abstract
        );

    // max stereotype text width (for width calculation)
    let stereotype_text_width = if has_stereotype_line {
        let kind_stereo_w = match entity.kind {
            EntityKind::Interface => font_metrics::text_width(
                "\u{00AB}interface\u{00BB}",
                "SansSerif",
                CLASS_FONT_SIZE,
                false,
                false,
            ),
            EntityKind::Enum => font_metrics::text_width(
                "\u{00AB}enum\u{00BB}",
                "SansSerif",
                CLASS_FONT_SIZE,
                false,
                false,
            ),
            EntityKind::Abstract => font_metrics::text_width(
                "\u{00AB}abstract\u{00BB}",
                "SansSerif",
                CLASS_FONT_SIZE,
                false,
                false,
            ),
            _ => 0.0,
        };
        let custom_stereo_w = entity
            .stereotypes
            .iter()
            .map(|s| {
                let stereo_text = format!("\u{00AB}{}\u{00BB}", s.0);
                font_metrics::text_width(&stereo_text, "SansSerif", CLASS_FONT_SIZE, false, false)
            })
            .fold(0.0_f64, f64::max);
        kind_stereo_w.max(custom_stereo_w)
    } else {
        0.0
    };

    // display text width for each member
    let max_member_width = entity
        .members
        .iter()
        .map(|m| {
            let mut member_text = String::new();
            if m.visibility.is_some() {
                member_text.push_str("+ "); // approximate visibility prefix
            }
            member_text.push_str(&m.name);
            if let Some(ref t) = m.return_type {
                member_text.push_str(": ");
                member_text.push_str(t);
            }
            font_metrics::text_width(&member_text, "SansSerif", CLASS_FONT_SIZE, false, false)
        })
        .fold(0.0_f64, f64::max);

    // Width: Java formula = circle_left_pad + circle_dia + gap + text_width + right_pad + generic
    let name_width =
        font_metrics::text_width(&name_display, "SansSerif", CLASS_FONT_SIZE, false, false);
    let gen_w = generic_dim_width(entity);
    let circle_plus_name =
        CIRCLE_LEFT_PAD + CIRCLE_DIAMETER + CIRCLE_TEXT_GAP + name_width + RIGHT_PAD + gen_w;
    let max_text_width = circle_plus_name
        .max(stereotype_text_width + CIRCLE_LEFT_PAD + RIGHT_PAD)
        .max(max_member_width + 2.0 * RIGHT_PAD);
    let width = max_text_width;

    // Height: Java formula = header(32) + fields_compartment + methods_compartment
    // Each compartment: empty=8, with N members = N * line_height + padding
    let _stereotype_extra = if has_stereotype_line {
        LINE_HEIGHT_PT
    } else {
        0.0
    };
    let fields_height = EMPTY_COMPARTMENT; // no field/method separation in our model yet
    let methods_height = if entity.members.is_empty() {
        EMPTY_COMPARTMENT
    } else {
        entity.members.len() as f64 * LINE_HEIGHT_PT + EMPTY_COMPARTMENT
    };
    let height = HEADER_HEIGHT_PT + fields_height + methods_height;

    log::debug!(
        "estimate_entity_size: {} -> ({}, {})",
        entity.name,
        width,
        height
    );

    (width, height)
}

fn estimate_members_width(members: &[&Member], font_size: f64) -> f64 {
    members
        .iter()
        .map(|m| {
            let text = member_text(m);
            let lines = split_member_lines(&text);
            let base_left = if m.visibility.is_some() {
                MEMBER_TEXT_LEFT_WITH_ICON
            } else {
                MEMBER_TEXT_LEFT_NO_ICON
            };
            lines
                .iter()
                .enumerate()
                .map(|(i, (line_text, indent))| {
                    let w = font_metrics::text_width(
                        line_text,
                        "SansSerif",
                        font_size,
                        false,
                        m.modifiers.is_abstract,
                    );
                    if i == 0 {
                        base_left + w
                    } else {
                        base_left + indent + w
                    }
                })
                .fold(0.0_f64, f64::max)
        })
        .fold(0.0_f64, f64::max)
}

fn section_height(show: bool, members: &[&Member], member_row_h: f64) -> f64 {
    if !show {
        return 0.0;
    }
    if members.is_empty() {
        return EMPTY_COMPARTMENT;
    }
    let total_visual_lines: usize = members.iter().map(|m| member_visual_lines(m)).sum();
    // Java: margin_top(4) + total_lines * member_row_height + margin_bottom(4)
    let one_row_h = member_row_h + 8.0;
    one_row_h + (total_visual_lines.saturating_sub(1)) as f64 * member_row_h
}

/// Java MemberImpl.getDisplay() format:
/// Uses raw display text when available (preserves original formatting).
/// Fallback: methods "name(): type", fields "name : type".
fn member_text(m: &Member) -> String {
    if let Some(ref display) = m.display {
        return display.clone();
    }
    match &m.return_type {
        Some(t) if m.name.ends_with(')') => format!("{}: {t}", m.name),
        Some(t) => format!("{} : {t}", m.name),
        None => m.name.clone(),
    }
}

/// Count the number of visual lines a member occupies.
fn member_visual_lines(m: &Member) -> usize {
    let text = member_text(m);
    split_member_lines(&text).len()
}

/// Split member display text into visual lines.
/// Splits on literal `\n` escape, U+E100 placeholder, and physical newlines.
/// Returns a vec of (trimmed_text, leading_space_width_at_14pt).
/// The first line always has indent=0; continuation lines use the width
/// of the leading whitespace as an indent offset from the first line.
pub(crate) fn split_member_lines(text: &str) -> Vec<(String, f64)> {
    let parts: Vec<&str> = text
        .split("\\n")
        .flat_map(|s| s.split(crate::NEWLINE_CHAR))
        .flat_map(|s| s.split('\n'))
        .collect();
    let mut result = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            result.push((part.to_string(), 0.0));
        } else {
            let trimmed = part.trim_start();
            let leading = &part[..part.len() - trimmed.len()];
            let indent =
                font_metrics::text_width(leading, "SansSerif", CLASS_FONT_SIZE, false, false);
            result.push((trimmed.to_string(), indent));
        }
    }
    result
}

fn show_portion(
    rules: &[ClassHideShowRule],
    portion: ClassPortion,
    entity_name: &str,
    member_count: usize,
) -> bool {
    let mut result = true;
    for rule in rules {
        if rule.portion != portion {
            continue;
        }
        // empty_only rules only apply when the section has no members
        if rule.empty_only && member_count > 0 {
            continue;
        }
        match &rule.target {
            ClassRuleTarget::Any => result = rule.show,
            ClassRuleTarget::Entity(name) if name == entity_name => result = rule.show,
            _ => {}
        }
    }
    result
}

/// Compute extra LimitFinder left extension for an entity's visibility modifier
/// polygons. Java's HACK_X_FOR_POLYGON=10 pushes polygon boundaries 10px left,
/// extending the LF min_x beyond the normal rect contribution.
///
/// For PROTECTED (diamond) and PACKAGE (triangle) modifiers, the polygon's
/// leftmost point is at node_x + 7 (MEMBER_ICON_X_OFFSET=6, then ox=x+1).
/// With HACK=10: LF sees node_x + 7 - 10 = node_x - 3.
/// Normal rect LF: node_x - 1. Extra = 3 - 1 = 2.
fn entity_lf_extra_left(cd: &ClassDiagram, entity: &Entity) -> f64 {
    let raw_field_count = entity.members.iter().filter(|m| !m.is_method).count();
    let raw_method_count = entity.members.iter().filter(|m| m.is_method).count();
    let show_fields = show_portion(
        &cd.hide_show_rules,
        ClassPortion::Field,
        &entity.name,
        raw_field_count,
    );
    let show_methods = show_portion(
        &cd.hide_show_rules,
        ClassPortion::Method,
        &entity.name,
        raw_method_count,
    );

    let has_polygon_modifier = entity.members.iter().any(|m| {
        let visible = if m.is_method {
            show_methods
        } else {
            show_fields
        };
        visible
            && matches!(
                m.visibility,
                Some(Visibility::Protected) | Some(Visibility::Package)
            )
    });

    if has_polygon_modifier {
        2.0
    } else {
        0.0
    }
}

fn visible_stereotype_labels(
    rules: &[ClassHideShowRule],
    stereotypes: &[Stereotype],
) -> Vec<String> {
    stereotypes
        .iter()
        .map(|st| {
            // Extract spot notation and return cleaned label
            let (_, cleaned) = st.extract_spot();
            cleaned
        })
        .filter(|label| !label.is_empty() && stereotype_label_visible(rules, label))
        .collect()
}

fn stereotype_label_visible(rules: &[ClassHideShowRule], label: &str) -> bool {
    let mut result = true;
    for rule in rules {
        if rule.portion != ClassPortion::Stereotype {
            continue;
        }
        match &rule.target {
            ClassRuleTarget::Any => result = rule.show,
            ClassRuleTarget::Stereotype(name) if name == label => result = rule.show,
            _ => {}
        }
    }
    result
}

/// Direction -> RankDir mapping
fn direction_to_rankdir(dir: &Direction) -> RankDir {
    match dir {
        Direction::TopToBottom => RankDir::TopToBottom,
        Direction::LeftToRight => RankDir::LeftToRight,
        Direction::BottomToTop => RankDir::BottomToTop,
        Direction::RightToLeft => RankDir::RightToLeft,
    }
}

/// Note font size
const NOTE_FONT_SIZE: f64 = 13.0;
/// SansSerif 13pt: ascent(12.0669) + descent(3.0659) = 15.1328
const NOTE_LINE_HEIGHT: f64 = 15.1328;
const NOTE_MARGIN_X1: f64 = 6.0;
const NOTE_MARGIN_X2: f64 = 15.0;
/// Java Opale marginY = 5
const NOTE_PADDING_Y: f64 = 5.0;
/// Gap between note and target entity
const NOTE_GAP: f64 = 16.0;

/// Compute the height of a single note text line, accounting for creole headings.
///
/// Java headings (`==text==`) use `bigger(2).bold()` which increases font size
/// and adds horizontal rule lines. Measurement from Java reference shows the
/// heading stripe is exactly 4px taller than a normal 13pt text line (19.1328
/// vs 15.1328).
fn note_line_height(line: &str) -> f64 {
    if crate::parser::creole::strip_heading_prefix_ordered(line).is_some() {
        NOTE_LINE_HEIGHT + 4.0
    } else {
        NOTE_LINE_HEIGHT
    }
}

/// Compute the text width of a note line, accounting for creole heading stripping.
///
/// Note headings (`==text==`) render at the same font size (13pt) as normal text
/// in Java, so we only strip the heading markers for width measurement.
fn note_line_width(line: &str) -> f64 {
    if let Some((rest, _order)) = crate::parser::creole::strip_heading_prefix_ordered(line) {
        font_metrics::text_width(rest.trim(), "SansSerif", NOTE_FONT_SIZE, false, false)
    } else {
        font_metrics::text_width(line, "SansSerif", NOTE_FONT_SIZE, false, false)
    }
}

/// Perform layout on a class diagram
fn layout_class_diagram(cd: &ClassDiagram, skin: &crate::style::SkinParams) -> Result<GraphLayout> {
    log::debug!(
        "layout_class_diagram: {} entities, {} links, {} notes",
        cd.entities.len(),
        cd.links.len(),
        cd.notes.len()
    );

    // Resolve font sizes from skinparams following Java's resolution order:
    // When classAttributeFontSize is set, it overrides classFontSize for both
    // header name and attributes (matching Java style priority).
    let explicit_attr_fs = skin
        .get("classattributefontsize")
        .and_then(|s| s.parse::<f64>().ok());
    let explicit_class_fs = skin
        .get("classfontsize")
        .and_then(|s| s.parse::<f64>().ok());
    let attr_font_size =
        explicit_attr_fs.unwrap_or_else(|| explicit_class_fs.unwrap_or(CLASS_FONT_SIZE));
    let name_font_size =
        explicit_attr_fs.unwrap_or_else(|| explicit_class_fs.unwrap_or(CLASS_FONT_SIZE));

    // Resolve member row height from skinparams.
    // Java default: FontParam.CLASS_ATTRIBUTE renders at 14pt (same as CLASS).
    // When classAttributeFontSize is explicitly set, use its line_height.
    let member_row_h: f64 = explicit_attr_fs
        .map(|sz| font_metrics::line_height("SansSerif", sz, false, false))
        .unwrap_or(MEMBER_ROW_HEIGHT);

    // build name -> sanitized id mapping
    let name_to_id: HashMap<String, String> = cd
        .entities
        .iter()
        .map(|e| (e.name.clone(), sanitize_id(&e.name)))
        .collect();
    let qualifier_margins = compute_entity_qualifier_margins(cd);

    // build LayoutNode list
    let mut nodes: Vec<LayoutNode> = cd
        .entities
        .iter()
        .map(|e| {
            let (mut w, h) =
                estimate_entity_size(cd, e, member_row_h, name_font_size, attr_font_size);
            let natural_w = w; // before qualifier expansion
            let shield = qualifier_margins.get(&e.name).and_then(|margins| {
                let kal_width = margins.up_total_width.max(margins.down_total_width);
                if kal_width > 0.0 {
                    w = w.max(kal_width * 1.3);
                }
                let shield = crate::svek::Margins::new(
                    margins.left,
                    margins.right,
                    margins.top,
                    margins.bottom,
                );
                if shield.is_zero() {
                    None
                } else {
                    Some(shield)
                }
            });
            // Java's HACK_X_FOR_POLYGON=10 extends the LimitFinder boundary
            // for visibility modifier polygons (PROTECTED/PACKAGE triangles/diamonds).
            // This pushes lf_min_x 2px beyond the rect's (node_x - 1).
            let lf_extra = entity_lf_extra_left(cd, e);
            // Java: EntityImageMap uses ShapeType.RECTANGLE_HTML_FOR_PORTS which
            // emits a shape=plaintext HTML table. Graphviz adds a default plaintext
            // margin of 0.055in ~= 4pt on top/bottom, inflating the node bbox by
            // ~8px total (4 above, 4 below). Mirror that by inflating the DOT node
            // height for Map entities; the renderer still uses the natural image
            // height from image_height_pt.
            let is_map = e.kind == EntityKind::Map && !e.map_entries.is_empty();
            let dot_h = if is_map { h + 8.0 } else { h };
            let natural_h = h;
            LayoutNode {
                id: name_to_id
                    .get(&e.name)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(&e.name)),
                label: e.name.clone(),
                width_pt: w,
                height_pt: dot_h,
                shape: None,
                shield,
                entity_position: None,
                max_label_width: None,
                port_label_width: None,
                order: e.source_line,
                image_width_pt: if (natural_w - w).abs() > 0.01 {
                    Some(natural_w)
                } else {
                    None
                },
                image_height_pt: if (natural_h - dot_h).abs() > 0.01 {
                    Some(natural_h)
                } else {
                    None
                },
                lf_extra_left: lf_extra,
                lf_rect_correction: true,
                lf_has_body_separator: false,
                lf_node_polygon: false,
                lf_polygon_hack: false,
                lf_actor_stickman: false,
                hidden: false,
            }
        })
        .collect();

    // build LayoutEdge list
    // Java: DotStringFactory uses minlen = link.getLength() - 1.
    // arrow_len=1 (single dash/dot) -> minlen=0 (same rank = horizontal).
    // arrow_len=2+ (double dash/dot) -> minlen=1+ (different ranks = vertical).
    let mut edges: Vec<LayoutEdge> = cd
        .links
        .iter()
        .map(|link| {
            let from_id = name_to_id
                .get(&link.from)
                .cloned()
                .unwrap_or_else(|| sanitize_id(&link.from));
            let to_id = name_to_id
                .get(&link.to)
                .cloned()
                .unwrap_or_else(|| sanitize_id(&link.to));
            LayoutEdge {
                from: from_id,
                to: to_id,
                label: link.label.clone(),
                label_dimension: None,
                tail_label: link.from_label.clone(),
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: link.to_label.clone(),
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration: arrow_head_to_svek_decoration(&link.left_head),
                head_decoration: arrow_head_to_svek_decoration(&link.right_head),
                line_style: link_style_to_svek(&link.line_style),
                minlen: link.arrow_len.saturating_sub(1) as u32,
                invisible: false,
                no_constraint: false,
            }
        })
        .collect();

    let standalone_by_container = collect_standalone_square_edges(cd, &name_to_id);
    edges.extend(standalone_by_container);

    // Java: notes are real entities (LeafType.NOTE) with GMN* IDs, connected
    // to their target via invisible dashed links. Add them as graphviz nodes
    // so they participate in layout and push entities to proper positions.
    let mut note_dot_ids: Vec<String> = Vec::new();
    for (i, note) in cd.notes.iter().enumerate() {
        let note_id = format!("GMN{}", i);
        let (nw, nh) = estimate_class_note_size(&note.text);
        nodes.push(LayoutNode {
            id: note_id.clone(),
            label: String::new(),
            width_pt: nw,
            height_pt: nh,
            shape: None,
            shield: None,
            entity_position: None,
            max_label_width: None,
            port_label_width: None,
            order: None,
            image_width_pt: None,
            image_height_pt: None,
            lf_extra_left: 0.0,
            lf_rect_correction: false,
            lf_has_body_separator: false,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: false,
        });
        // Invisible edge between note and target entity (Java link pattern).
        if let Some(ref target) = note.target {
            let target_id = name_to_id
                .get(target)
                .cloned()
                .unwrap_or_else(|| sanitize_id(target));
            // Java: LEFT → note→target (length=1, same rank), RIGHT → target→note (length=1, same rank)
            //        TOP → note→target (length=2, vertical), BOTTOM → target→note (length=2, vertical)
            let (from, to, minlen) = match note.position.as_str() {
                "top" => (note_id.clone(), target_id, 1),
                "bottom" => (target_id, note_id.clone(), 1),
                "left" => (note_id.clone(), target_id, 0),
                "right" => (target_id, note_id.clone(), 0),
                _ => (note_id.clone(), target_id, 1),
            };
            let no_constraint = matches!(note.position.as_str(), "left" | "right");
            edges.push(LayoutEdge {
                from,
                to,
                label: None,
                label_dimension: None,
                tail_label: None,
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration: crate::svek::edge::LinkDecoration::None,
                head_decoration: crate::svek::edge::LinkDecoration::None,
                line_style: crate::svek::edge::LinkStyle::Dashed,
                minlen,
                invisible: true,
                no_constraint,
            });
        }
        note_dot_ids.push(note_id);
    }

    // Java: rankdir=LR is only emitted when `left to right direction` was explicitly written.
    // When direction is inferred from arrow length, rankdir stays TB (default) and
    // layout is controlled via edge minlen values.
    let rankdir = if cd.direction_explicit {
        direction_to_rankdir(&cd.direction)
    } else {
        RankDir::TopToBottom
    };
    let clusters = build_layout_clusters(cd, &name_to_id);

    let graph = LayoutGraph {
        nodes,
        edges,
        clusters,
        rankdir,
        is_activity: false,
        ranksep_override: None,
        nodesep_override: None,
        use_simplier_dot_link_strategy: true,
        arrow_font_size: None,
    };

    let mut layout = layout_with_svek(&graph)?;

    // Expand total_width/total_height to include edge label extents.
    // Java: LimitFinder.ensureVisible tracks all drawn elements including text.
    // Edge labels are drawn at the edge midpoint; their text can extend beyond nodes.
    let link_label_font_size = 13.0_f64; // Java: FontParam.CLASS uses 13pt for link labels
    for el in &layout.edges {
        if let Some(ref label) = el.label {
            if el.points.is_empty() {
                continue;
            }
            let mid_idx = el.points.len() / 2;
            let (mx, _my) = el.points[mid_idx];
            // Label is drawn at mx+1 (1px offset in draw_label), extending right
            let lines: Vec<&str> = label
                .split("\\n")
                .flat_map(|s| s.split(crate::NEWLINE_CHAR))
                .flat_map(|s| s.split("\\l"))
                .flat_map(|s| s.split("\\r"))
                .collect();
            let max_line_w = lines
                .iter()
                .map(|l| {
                    font_metrics::text_width(l, "SansSerif", link_label_font_size, false, false)
                })
                .fold(0.0_f64, f64::max);
            let label_right = mx + 1.0 + max_line_w;
            if label_right > layout.total_width {
                layout.total_width = label_right;
            }
        }
    }

    // Compute note layouts using graphviz positions for notes that participated
    // in the graphviz solve (GMN* nodes), falling back to entity-relative placement.
    layout.notes = compute_note_layouts(&cd.notes, &layout.nodes, &name_to_id, &note_dot_ids);

    // expand total_width / total_height to accommodate notes
    for note in &layout.notes {
        let right_edge = note.x + note.width;
        let bottom_edge = note.y + note.height;
        if right_edge > layout.total_width {
            layout.total_width = right_edge;
        }
        if bottom_edge > layout.total_height {
            layout.total_height = bottom_edge;
        }
    }
    // notes may produce negative coordinates on left or top, shift if needed
    let min_x = layout.notes.iter().map(|n| n.x).fold(0.0_f64, f64::min);
    let min_y = layout.notes.iter().map(|n| n.y).fold(0.0_f64, f64::min);
    if min_x < 0.0 || min_y < 0.0 {
        let shift_x = if min_x < 0.0 { -min_x } else { 0.0 };
        let shift_y = if min_y < 0.0 { -min_y } else { 0.0 };
        for n in &mut layout.nodes {
            n.cx += shift_x;
            n.cy += shift_y;
        }
        for e in &mut layout.edges {
            for pt in &mut e.points {
                pt.0 += shift_x;
                pt.1 += shift_y;
            }
            if let Some(ref mut tip) = e.arrow_tip {
                tip.0 += shift_x;
                tip.1 += shift_y;
            }
            if let Some(ref raw_d) = e.raw_path_d {
                e.raw_path_d = Some(graphviz::transform_path_d(raw_d, shift_x, shift_y));
            }
            if let Some(ref mut pts) = e.arrow_polygon_points {
                for p in pts.iter_mut() {
                    p.0 += shift_x;
                    p.1 += shift_y;
                }
            }
            if let Some(ref mut xy) = e.tail_label_xy {
                xy.0 += shift_x;
                xy.1 += shift_y;
            }
            if let Some(ref mut xy) = e.head_label_xy {
                xy.0 += shift_x;
                xy.1 += shift_y;
            }
        }
        for n in &mut layout.notes {
            n.x += shift_x;
            n.y += shift_y;
            if let Some(ref mut conn) = n.connector {
                conn.0 += shift_x;
                conn.1 += shift_y;
                conn.2 += shift_x;
                conn.3 += shift_y;
            }
        }
        layout.total_width += shift_x;
        layout.total_height += shift_y;
    }

    Ok(layout)
}

fn collect_standalone_square_edges(
    cd: &ClassDiagram,
    name_to_id: &HashMap<String, String>,
) -> Vec<LayoutEdge> {
    let mut result = Vec::new();

    let linked_entities: std::collections::HashSet<&str> = cd
        .links
        .iter()
        .flat_map(|link| [link.from.as_str(), link.to.as_str()])
        .collect();

    let grouped_entities: std::collections::HashSet<&str> = cd
        .groups
        .iter()
        .flat_map(|group| group.entities.iter().map(String::as_str))
        .collect();

    let root_standalones: Vec<&str> = cd
        .entities
        .iter()
        .map(|entity| entity.name.as_str())
        .filter(|name| !linked_entities.contains(name))
        .filter(|name| !grouped_entities.contains(name))
        .collect();
    result.extend(square_edges_for_entities(&root_standalones, name_to_id));

    for group in &cd.groups {
        let standalones: Vec<&str> = group
            .entities
            .iter()
            .map(String::as_str)
            .filter(|name| !linked_entities.contains(name))
            .collect();
        result.extend(square_edges_for_entities(&standalones, name_to_id));
    }

    result
}

fn square_edges_for_entities(
    entity_names: &[&str],
    name_to_id: &HashMap<String, String>,
) -> Vec<LayoutEdge> {
    if entity_names.len() < 3 {
        return Vec::new();
    }

    let branch = compute_square_branch(entity_names.len());
    let ids: Vec<String> = entity_names
        .iter()
        .map(|name| {
            name_to_id
                .get(*name)
                .cloned()
                .unwrap_or_else(|| sanitize_id(name))
        })
        .collect();

    let mut result = Vec::new();
    let mut head_branch = 0usize;
    for i in 1..ids.len() {
        let dist = i - head_branch;
        if dist == branch {
            result.push(LayoutEdge {
                from: ids[head_branch].clone(),
                to: ids[i].clone(),
                label: None,
                label_dimension: None,
                tail_label: None,
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration: crate::svek::edge::LinkDecoration::None,
                head_decoration: crate::svek::edge::LinkDecoration::None,
                line_style: crate::svek::edge::LinkStyle::Normal,
                minlen: 1,
                invisible: true,
                no_constraint: false,
            });
            head_branch = i;
        } else {
            result.push(LayoutEdge {
                from: ids[i - 1].clone(),
                to: ids[i].clone(),
                label: None,
                label_dimension: None,
                tail_label: None,
                tail_label_dimension: None,
                tail_label_boxed: false,
                head_label: None,
                head_label_dimension: None,
                head_label_boxed: false,
                tail_decoration: crate::svek::edge::LinkDecoration::None,
                head_decoration: crate::svek::edge::LinkDecoration::None,
                line_style: crate::svek::edge::LinkStyle::Normal,
                minlen: 0,
                invisible: true,
                no_constraint: false,
            });
        }
    }

    result
}

fn compute_square_branch(size: usize) -> usize {
    let sqrt = (size as f64).sqrt() as usize;
    if sqrt * sqrt == size {
        sqrt
    } else {
        sqrt + 1
    }
}

/// Estimate note size accounting for embedded subdiagrams.
fn estimate_class_note_size(text: &str) -> (f64, f64) {
    if let Some(block) = crate::render::embedded::extract_embedded(text) {
        if let Some((_, ew, eh)) =
            crate::render::embedded::render_embedded(&block.inner_source, &block.diagram_type)
        {
            let before_lines: Vec<&str> = if block.before.is_empty() {
                vec![]
            } else {
                block.before.lines().collect()
            };
            let after_lines: Vec<&str> = if block.after.is_empty() {
                // Check for trailing newline after `}}` in the original text.
                // Java counts this as one blank line for note height.
                if text.trim_end().ends_with("}}") && text.ends_with('\n') {
                    vec![""]
                } else {
                    vec![]
                }
            } else {
                block.after.lines().collect()
            };
            let has_heading = before_lines
                .iter()
                .chain(after_lines.iter())
                .any(|l| crate::parser::creole::strip_heading_prefix_ordered(l).is_some());
            let before_w: f64 = before_lines
                .iter()
                .map(|l| note_line_width(l))
                .fold(0.0_f64, f64::max);
            let after_w: f64 = after_lines
                .iter()
                .map(|l| note_line_width(l))
                .fold(0.0_f64, f64::max);
            let content_w = before_w.max(ew).max(after_w);
            // Java: heading horizontal rule stripes add 6px extra width
            let heading_extra = if has_heading { 6.0 } else { 0.0 };
            let w = (content_w + NOTE_MARGIN_X1 + NOTE_MARGIN_X2 + heading_extra).max(60.0);
            let before_h: f64 = before_lines.iter().map(|l| note_line_height(l)).sum();
            let after_h: f64 = after_lines.iter().map(|l| note_line_height(l)).sum();
            let h = before_h + eh + after_h + NOTE_PADDING_Y * 2.0;
            return (w, h);
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let max_line_width = lines
        .iter()
        .map(|l| font_metrics::text_width(l, "SansSerif", NOTE_FONT_SIZE, false, false))
        .fold(0.0_f64, f64::max);
    let w = (max_line_width + NOTE_MARGIN_X1 + NOTE_MARGIN_X2).max(60.0);
    let h = lines.len() as f64 * NOTE_LINE_HEIGHT + NOTE_PADDING_Y * 2.0;
    (w, h)
}

/// Compute note layout positions
fn compute_note_layouts(
    notes: &[crate::model::ClassNote],
    nodes: &[graphviz::NodeLayout],
    name_to_id: &HashMap<String, String>,
    note_dot_ids: &[String],
) -> Vec<graphviz::ClassNoteLayout> {
    let node_map: HashMap<&str, &graphviz::NodeLayout> =
        nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    notes
        .iter()
        .enumerate()
        .map(|(i, note)| {
            // Detect and render embedded diagrams in note text
            let embedded = crate::render::embedded::extract_embedded(&note.text).and_then(|block| {
                crate::render::embedded::render_embedded(&block.inner_source, &block.diagram_type).map(
                    |(inner_svg, w, h)| crate::layout::component::EmbeddedDiagramData {
                        data_uri: crate::render::embedded::svg_to_data_uri(&inner_svg),
                        width: w,
                        height: h,
                        text_before: block.before,
                        text_after: block.after,
                    },
                )
            });

            let (note_width, note_height, lines) = if let Some(ref emb) = embedded {
                let before_lines: Vec<String> = if emb.text_before.is_empty() {
                    vec![]
                } else {
                    emb.text_before.lines().map(String::from).collect()
                };
                let after_lines: Vec<String> = if emb.text_after.is_empty() {
                    // Check if the original note text has a trailing newline after `}}`.
                    // Java counts this as one blank line for note height calculation.
                    if note.text.trim_end().ends_with("}}") && note.text.ends_with('\n') {
                        vec![String::new()]
                    } else {
                        vec![]
                    }
                } else {
                    emb.text_after.lines().map(String::from).collect()
                };
                let before_w: f64 = before_lines
                    .iter()
                    .map(|l| note_line_width(l))
                    .fold(0.0_f64, f64::max);
                let after_w: f64 = after_lines
                    .iter()
                    .map(|l| note_line_width(l))
                    .fold(0.0_f64, f64::max);
                let has_heading = before_lines.iter().chain(after_lines.iter())
                    .any(|l| crate::parser::creole::strip_heading_prefix_ordered(l).is_some());
                let content_w = before_w.max(emb.width).max(after_w);
                let heading_extra = if has_heading { 6.0 } else { 0.0 };
                let w = (content_w + NOTE_MARGIN_X1 + NOTE_MARGIN_X2 + heading_extra).max(60.0);
                let before_h: f64 = before_lines.iter().map(|l| note_line_height(l)).sum();
                let after_h: f64 = after_lines.iter().map(|l| note_line_height(l)).sum();
                let h = before_h + emb.height + after_h + NOTE_PADDING_Y * 2.0;
                log::debug!("embedded note layout: before_h={before_h}, emb_h={}, after_h={after_h}, total_h={h}, w={w}, before_lines={:?}", emb.height, before_lines);
                let all_lines: Vec<String> = before_lines.into_iter()
                    .chain(std::iter::once("{{embedded}}".to_string()))
                    .chain(after_lines)
                    .collect();
                (w, h, all_lines)
            } else {
                let lines: Vec<String> = note
                    .text
                    .lines()
                    .map(std::string::ToString::to_string)
                    .collect();
                let max_line_width = lines
                    .iter()
                    .map(|l| font_metrics::text_width(l, "SansSerif", NOTE_FONT_SIZE, false, false))
                    .fold(0.0_f64, f64::max);
                let w = (max_line_width + NOTE_MARGIN_X1 + NOTE_MARGIN_X2).max(60.0);
                let h = lines.len() as f64 * NOTE_LINE_HEIGHT + NOTE_PADDING_Y * 2.0;
                (w, h, lines)
            };

            // Use graphviz position for this note if available
            let note_gv_node = note_dot_ids.get(i).and_then(|nid| node_map.get(nid.as_str()).copied());
            // Find target entity node
            let target_node = note.target.as_ref().and_then(|target| {
                let sid = name_to_id
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| sanitize_id(target));
                node_map.get(sid.as_str()).copied()
            });

            // Ear-tip offsets for top/bottom notes. Java Opale uses Smetana
            // spline endpoints which land slightly inside the entity bounding
            // box; these small deltas match the component renderer values
            // (see src/layout/component.rs near ear_tip_y computation).
            const TOP_EAR_OFFSET: f64 = 0.23;
            const BOTTOM_EAR_OFFSET: f64 = 0.123125;

            let (x, y, connector) = if let Some(gv_note) = note_gv_node {
                // Use graphviz-determined position (top-left corner)
                let nx = gv_note.cx - gv_note.width / 2.0;
                let ny = gv_note.cy - gv_note.height / 2.0;
                // Compute connector to target entity
                let conn = target_node.map(|nl| {
                    let note_right = nx + note_width;
                    let note_cx = nx + note_width / 2.0;
                    let note_cy = ny + note_height / 2.0;
                    let entity_left = nl.cx - nl.width / 2.0;
                    let entity_right = nl.cx + nl.width / 2.0;
                    let entity_top = nl.cy - nl.height / 2.0;
                    let entity_bottom = nl.cy + nl.height / 2.0;
                    match note.position.as_str() {
                        "left" => (note_right, note_cy, entity_left, note_cy),
                        "right" => (nx, note_cy, entity_right, note_cy),
                        "top" => (
                            note_cx,
                            ny + note_height,
                            nl.cx,
                            entity_top - TOP_EAR_OFFSET,
                        ),
                        "bottom" => (
                            note_cx,
                            ny,
                            nl.cx,
                            entity_bottom + BOTTOM_EAR_OFFSET,
                        ),
                        _ => (note_right, note_cy, entity_left, note_cy),
                    }
                });
                (nx, ny, conn)
            } else if let Some(nl) = target_node {
                // Fallback: position relative to target entity
                let entity_left = nl.cx - nl.width / 2.0;
                let entity_right = nl.cx + nl.width / 2.0;
                let entity_top = nl.cy - nl.height / 2.0;
                let entity_bottom = nl.cy + nl.height / 2.0;
                let entity_center_y = nl.cy;

                match note.position.as_str() {
                    "right" => {
                        let nx = entity_right + NOTE_GAP;
                        let ny = entity_center_y - note_height / 2.0;
                        let conn = (nx, entity_center_y, entity_right, entity_center_y);
                        (nx, ny, Some(conn))
                    }
                    "left" => {
                        let nx = entity_left - NOTE_GAP - note_width;
                        let ny = entity_center_y - note_height / 2.0;
                        let conn = (
                            nx + note_width,
                            entity_center_y,
                            entity_left,
                            entity_center_y,
                        );
                        (nx, ny, Some(conn))
                    }
                    "top" => {
                        let nx = nl.cx - note_width / 2.0;
                        let ny = entity_top - NOTE_GAP - note_height;
                        let conn = (nl.cx, ny + note_height, nl.cx, entity_top - TOP_EAR_OFFSET);
                        (nx, ny, Some(conn))
                    }
                    "bottom" => {
                        let nx = nl.cx - note_width / 2.0;
                        let ny = entity_bottom + NOTE_GAP;
                        let conn = (nl.cx, ny, nl.cx, entity_bottom + BOTTOM_EAR_OFFSET);
                        (nx, ny, Some(conn))
                    }
                    _ => {
                        let nx = entity_right + NOTE_GAP;
                        let ny = entity_center_y - note_height / 2.0;
                        let conn = (nx, entity_center_y, entity_right, entity_center_y);
                        (nx, ny, Some(conn))
                    }
                }
            } else {
                // no target entity, place at a floating position near bottom-right
                let max_x = nodes
                    .iter()
                    .filter(|n| !n.id.starts_with("GMN"))
                    .map(|n| n.cx + n.width / 2.0)
                    .fold(0.0_f64, f64::max);
                let max_y = nodes
                    .iter()
                    .filter(|n| !n.id.starts_with("GMN"))
                    .map(|n| n.cy + n.height / 2.0)
                    .fold(0.0_f64, f64::max);
                (max_x + NOTE_GAP, max_y + NOTE_GAP, None)
            };

            graphviz::ClassNoteLayout {
                text: note.text.clone(),
                x,
                y,
                width: note_width,
                height: note_height,
                lines,
                connector,
                embedded,
                position: note.position.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, EntityKind, Member, MemberModifiers, Visibility};
    use std::collections::HashMap;

    fn empty_entity(name: &str) -> Entity {
        Entity {
            name: name.to_string(),
            ..Entity::default()
        }
    }

    fn make_member(vis: Option<Visibility>, name: &str, ret: Option<&str>) -> Member {
        Member {
            visibility: vis,
            name: name.to_string(),
            return_type: ret.map(|s| s.to_string()),
            is_method: false,
            modifiers: MemberModifiers::default(),
            display: None,
        }
    }

    fn empty_diagram() -> ClassDiagram {
        ClassDiagram {
            entities: vec![],
            links: vec![],
            groups: vec![],
            direction: Direction::TopToBottom,
            direction_explicit: false,
            notes: vec![],
            hide_show_rules: vec![],
            stereotype_backgrounds: HashMap::new(),
        }
    }

    #[test]
    fn estimate_size_empty_class_returns_minimum() {
        let e = empty_entity("Foo");
        let (w, h) = estimate_entity_size(
            &empty_diagram(),
            &e,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        // Width = circle(4+22) + gap(3) + text_width("Foo",14) + pad(3) ≈ 57
        assert!(w >= 40.0, "width should be >= 40, got {w}");
        // Height = header(32) + fields(8) + methods(8) = 48
        assert!(h >= 48.0, "height should be >= 48, got {h}");
    }

    #[test]
    fn estimate_size_accounts_for_members() {
        let e = Entity {
            name: "A".to_string(),
            members: vec![
                make_member(
                    Some(Visibility::Private),
                    "longFieldNameHere",
                    Some("String"),
                ),
                make_member(Some(Visibility::Public), "id", Some("i32")),
            ],
            ..Entity::default()
        };
        let (w, h) = estimate_entity_size(
            &empty_diagram(),
            &e,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );

        // height = header(32) + fields(8) + members(2*8+8) = 64
        let expected_min_height =
            HEADER_HEIGHT_PT + EMPTY_COMPARTMENT + 2.0 * LINE_HEIGHT_PT + EMPTY_COMPARTMENT;
        assert!(
            h >= expected_min_height,
            "height {h} should be >= {expected_min_height}"
        );

        let member_text = "- longFieldNameHere: String";
        let expected_min_width = crate::font_metrics::text_width(
            member_text,
            "SansSerif",
            CLASS_ATTR_FONT_SIZE,
            false,
            false,
        ) + 2.0 * RIGHT_PAD;
        assert!(
            w >= expected_min_width,
            "width {w} should be >= {expected_min_width}"
        );
    }

    #[test]
    fn estimate_size_interface_uses_standard_header_height() {
        let e = Entity {
            name: "Runnable".to_string(),
            kind: EntityKind::Interface,
            ..Entity::default()
        };
        let (_, h) = estimate_entity_size(
            &empty_diagram(),
            &e,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );

        let expected = HEADER_HEIGHT_PT + 2.0 * EMPTY_COMPARTMENT;
        assert_eq!(
            h, expected,
            "interface height should follow the standard class header path"
        );
    }

    #[test]
    fn estimate_size_with_generic_widens() {
        let plain = empty_entity("Map");
        let generic = Entity {
            generic: Some("K, V".to_string()),
            ..plain.clone()
        };
        let diagram = empty_diagram();
        let (w_plain, _) = estimate_entity_size(
            &diagram,
            &plain,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        let (w_generic, _) = estimate_entity_size(
            &diagram,
            &generic,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        assert!(
            w_generic > w_plain,
            "generic entity should be wider: {w_generic} > {w_plain}"
        );
    }

    #[test]
    fn sanitize_id_escapes_special_chars() {
        assert_eq!(sanitize_id("List<String>"), "List_LT_String_GT_");
        assert_eq!(sanitize_id("Map<K, V>"), "Map_LT_K_COMMA__V_GT_");
        assert_eq!(sanitize_id("pkg1.pkg2.Class"), "pkg1_DOT_pkg2_DOT_Class");
        assert_eq!(sanitize_id("Simple"), "Simple");
        assert_eq!(sanitize_id("My Class"), "My_Class");
    }

    #[test]
    fn direction_maps_to_rankdir() {
        assert!(matches!(
            direction_to_rankdir(&Direction::TopToBottom),
            RankDir::TopToBottom
        ));
        assert!(matches!(
            direction_to_rankdir(&Direction::LeftToRight),
            RankDir::LeftToRight
        ));
        assert!(matches!(
            direction_to_rankdir(&Direction::BottomToTop),
            RankDir::BottomToTop
        ));
        assert!(matches!(
            direction_to_rankdir(&Direction::RightToLeft),
            RankDir::RightToLeft
        ));
    }

    #[test]
    fn note_position_right_of_entity() {
        use crate::model::ClassNote;

        let nodes = vec![graphviz::NodeLayout {
            id: "Foo".into(),
            cx: 100.0,
            cy: 50.0,
            width: 120.0,
            height: 80.0,
            image_width: 120.0,
            min_x: 40.0,
            min_y: 10.0,
        }];
        let name_to_id: HashMap<String, String> = [("Foo".to_string(), "Foo".to_string())]
            .into_iter()
            .collect();
        let notes = vec![ClassNote {
            text: "hello".to_string(),
            position: "right".to_string(),
            target: Some("Foo".to_string()),
        }];

        let result = compute_note_layouts(&notes, &nodes, &name_to_id, &[]);
        assert_eq!(result.len(), 1);
        let nl = &result[0];
        // note x should be past entity right edge + gap
        let entity_right = 100.0 + 120.0 / 2.0; // 160
        assert!(
            nl.x >= entity_right,
            "note x={} should be >= entity_right={}",
            nl.x,
            entity_right
        );
        assert!(nl.width > 0.0);
        assert!(nl.height > 0.0);
        assert!(nl.connector.is_some());
    }

    #[test]
    fn note_position_left_of_entity() {
        use crate::model::ClassNote;

        let nodes = vec![graphviz::NodeLayout {
            id: "Bar".into(),
            cx: 200.0,
            cy: 100.0,
            width: 100.0,
            height: 60.0,
            image_width: 100.0,
            min_x: 150.0,
            min_y: 70.0,
        }];
        let name_to_id: HashMap<String, String> = [("Bar".to_string(), "Bar".to_string())]
            .into_iter()
            .collect();
        let notes = vec![ClassNote {
            text: "left note".to_string(),
            position: "left".to_string(),
            target: Some("Bar".to_string()),
        }];

        let result = compute_note_layouts(&notes, &nodes, &name_to_id, &[]);
        assert_eq!(result.len(), 1);
        let nl = &result[0];
        let entity_left = 200.0 - 100.0 / 2.0; // 150
                                               // note right edge should be before entity left edge
        assert!(
            nl.x + nl.width <= entity_left,
            "note right edge={} should be <= entity_left={}",
            nl.x + nl.width,
            entity_left
        );
        assert!(nl.connector.is_some());
    }

    #[test]
    fn note_without_target_floats() {
        use crate::model::ClassNote;

        let nodes = vec![graphviz::NodeLayout {
            id: "X".into(),
            cx: 50.0,
            cy: 50.0,
            width: 80.0,
            height: 40.0,
            image_width: 80.0,
            min_x: 10.0,
            min_y: 30.0,
        }];
        let name_to_id: HashMap<String, String> =
            [("X".to_string(), "X".to_string())].into_iter().collect();
        let notes = vec![ClassNote {
            text: "floating".to_string(),
            position: "right".to_string(),
            target: None,
        }];

        let result = compute_note_layouts(&notes, &nodes, &name_to_id, &[]);
        assert_eq!(result.len(), 1);
        assert!(
            result[0].connector.is_none(),
            "floating note should have no connector"
        );
    }

    #[test]
    fn split_name_single_line() {
        let lines = split_name_display_lines("Foo");
        assert_eq!(lines, vec!["Foo"]);
    }

    #[test]
    fn split_name_backslash_n() {
        let lines = split_name_display_lines("Class 1\\nLine 2");
        assert_eq!(lines, vec!["Class 1", "Line 2"]);
    }

    #[test]
    fn split_name_backslash_r_n() {
        let lines = split_name_display_lines("Class 1\\r\\nLine 2");
        assert_eq!(lines, vec!["Class 1", "", "Line 2"]);
    }

    #[test]
    fn split_name_backslash_r() {
        let lines = split_name_display_lines("Part A\\rPart B");
        assert_eq!(lines, vec!["Part A", "Part B"]);
    }

    #[test]
    fn split_name_crlf_preserves_empty_line_in_visible_lines() {
        let lines = split_name_display_lines("Before\\r\\n\\tAfter");
        assert_eq!(lines, vec!["Before", "", "After"]);
    }

    #[test]
    fn split_name_display_tracks_alignment_and_leading_tabs() {
        let block = split_name_display("Before\\r\\n\\tAfter");
        assert_eq!(block.alignment, DisplayAlignment::Right);
        assert_eq!(
            block.lines,
            vec![
                DisplayLine {
                    text: "Before".to_string(),
                    leading_tabs: 0
                },
                DisplayLine {
                    text: "".to_string(),
                    leading_tabs: 0
                },
                DisplayLine {
                    text: "After".to_string(),
                    leading_tabs: 1
                },
            ]
        );
    }

    #[test]
    fn display_line_metrics_counts_tab_indent_separately() {
        let line = DisplayLine {
            text: "After".to_string(),
            leading_tabs: 1,
        };
        let (visible_width, indent_width) = display_line_metrics(&line, 14.0, false, false);
        let expected_visible = font_metrics::text_width("After", "SansSerif", 14.0, false, false);
        let expected_indent = display_tab_width(14.0, false, false);
        assert!((visible_width - expected_visible).abs() < 1e-9);
        assert!((indent_width - expected_indent).abs() < 1e-9);
    }

    #[test]
    fn split_name_separate_r_and_n() {
        // \r and \n as separate sequences produce empty line between
        let lines = split_name_display_lines("A\\rB\\nC");
        assert_eq!(lines, vec!["A", "B", "C"]);
    }

    #[test]
    fn split_name_newline_char() {
        let name = format!("A{}B", crate::NEWLINE_CHAR);
        let lines = split_name_display_lines(&name);
        assert_eq!(lines, vec!["A", "B"]);
    }

    #[test]
    fn class_entity_display_name_drops_namespace_prefix() {
        let display = class_entity_display_name("pkg1.pkg2.Class 1\\r\\n\\tBody");
        assert_eq!(display, "Class 1\\r\\n\\tBody");
    }

    #[test]
    fn multiline_name_increases_header_height() {
        let single = empty_entity("Foo");
        let multi = Entity {
            name: "Foo\\nBar".to_string(),
            ..single.clone()
        };
        let diagram = empty_diagram();
        let (_, h_single) = estimate_entity_size(
            &diagram,
            &single,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        let (_, h_multi) = estimate_entity_size(
            &diagram,
            &multi,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        // Multi-line name should produce a taller entity
        assert!(
            h_multi > h_single,
            "multi-line name height {h_multi} should be > single-line {h_single}"
        );
        // With HEADER_CIRCLE_BLOCK_HEIGHT(32) as minimum:
        // single: header = max(32, 16.297 + 10) = 32, total = 32 + 16 = 48
        // double: header = max(32, 32.594 + 10) = 42.594, total = 42.594 + 16 = 58.594
        // diff ≈ 10.594 (not full line_height because circle_block caps single-line header)
        let diff = h_multi - h_single;
        assert!(
            diff > 10.0 && diff < 17.0,
            "height diff {diff} should be between 10 and 17 (name line height minus circle cap)"
        );
    }

    #[test]
    fn three_line_name_height() {
        let single = empty_entity("A");
        let triple = Entity {
            name: "A\\nB\\nC".to_string(),
            ..single.clone()
        };
        let diagram = empty_diagram();
        let (_, h_single) = estimate_entity_size(
            &diagram,
            &single,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        let (_, h_triple) = estimate_entity_size(
            &diagram,
            &triple,
            MEMBER_ROW_HEIGHT,
            CLASS_FONT_SIZE,
            CLASS_FONT_SIZE,
        );
        // Three-line name: header = max(32, 3*16.297 + 10) = max(32, 58.891) = 58.891
        // vs single-line: header = 32
        // diff = 58.891 - 32 = 26.891
        let diff = h_triple - h_single;
        assert!(
            (diff - 26.890625).abs() < 0.01,
            "3-line vs 1-line height diff {diff} should be ~26.891"
        );
    }
}
