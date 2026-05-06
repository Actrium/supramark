//! ERD (Chen notation) layout engine.
//!
//! Converts an `ErdDiagram` into a fully positioned `ErdLayout` ready for SVG
//! rendering.  Assigns ranks via BFS over the link graph so that connected
//! nodes form chains, then spreads each rank along the cross-axis.

use std::collections::HashMap;

use log::debug;

use crate::font_metrics;
use crate::layout::graphviz::{self, LayoutEdge, LayoutGraph, LayoutNode, RankDir};
use crate::model::erd::{ErdAttribute, ErdDiagram, ErdDirection, ErdLink};
use crate::render::svg::ViewportConfig;
use crate::svek::shape_type::ShapeType;
use crate::Result;

// ---------------------------------------------------------------------------
// Layout output types
// ---------------------------------------------------------------------------

/// Fully positioned ERD ready for rendering.
#[derive(Debug)]
pub struct ErdLayout {
    pub entity_nodes: Vec<ErdNodeLayout>,
    pub relationship_nodes: Vec<ErdNodeLayout>,
    pub attribute_nodes: Vec<ErdAttrLayout>,
    pub edges: Vec<ErdEdgeLayout>,
    /// Attribute→parent connection paths from graphviz edge routing.
    pub attr_edges: Vec<ErdAttrEdge>,
    pub isa_layouts: Vec<ErdIsaLayout>,
    pub notes: Vec<ErdNoteLayout>,
    pub width: f64,
    pub height: f64,
    /// Map from node ID to svek uid (0-based index in DOT node order).
    /// Used for generating `entXXXX` and `lnkXX` indices in SVG output.
    pub svek_node_uids: HashMap<String, usize>,
    /// Map from link source_order to its uid in the DOT model.
    pub link_uids: HashMap<usize, usize>,
    /// Map from link source_order to its 0-indexed source line in the full source.
    pub link_source_lines: HashMap<usize, usize>,
}

/// A graphviz-routed edge connecting an attribute to its parent.
#[derive(Debug, Clone)]
pub struct ErdAttrEdge {
    pub raw_path_d: Option<String>,
    pub from_name: String,
    pub to_name: String,
    /// Source order of the parent entity/relationship that owns this attribute.
    pub parent_source_order: usize,
    /// Per-attribute line color for the edge stroke.
    pub edge_color: Option<String>,
    /// 0-based source line of the attribute declaration.
    pub attr_source_line: usize,
}

/// A positioned entity or relationship node.
#[derive(Debug, Clone)]
pub struct ErdNodeLayout {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_weak: bool,
    pub is_identifying: bool,
    /// True if this is a relationship (diamond), false if entity (rectangle).
    pub is_relationship: bool,
    /// Source declaration order (shared counter between entities and relationships).
    pub source_order: usize,
    /// Per-entity background color override (from `#color` syntax).
    pub bg_color: Option<String>,
    /// Per-entity border/line color override (from `line:color` syntax).
    pub line_color: Option<String>,
}

/// A positioned attribute ellipse.
#[derive(Debug, Clone)]
pub struct ErdAttrLayout {
    pub id: String,
    pub label: String,
    pub parent: String,
    pub x: f64,
    pub y: f64,
    pub rx: f64,
    pub ry: f64,
    pub is_key: bool,
    pub is_derived: bool,
    pub is_multi: bool,
    pub has_type: bool,
    pub type_label: Option<String>,
    /// Sub-attributes for nested attributes
    pub children: Vec<ErdAttrLayout>,
    /// Per-attribute background color override (from `#color` syntax).
    pub bg_color: Option<String>,
    /// Per-attribute border/line color override (from `line:color` syntax).
    pub line_color: Option<String>,
}

/// An edge connecting two positioned elements.
#[derive(Debug, Clone)]
pub struct ErdEdgeLayout {
    pub from_id: String,
    pub to_id: String,
    pub from_name: String,
    pub to_name: String,
    pub from_point: (f64, f64),
    pub to_point: (f64, f64),
    pub label: String,
    pub is_double: bool,
    pub source_line: usize,
    pub entity_idx_from: usize,
    pub entity_idx_to: usize,
    /// Raw SVG path d-string from graphviz (via svek pipeline).
    pub raw_path_d: Option<String>,
    /// Label position from svek solve (x, y).
    pub label_xy: Option<(f64, f64)>,
    /// Source declaration order of the originating link.
    pub source_order: usize,
    /// ISA arrow direction: None for normal links, Some(true) for `>`, Some(false) for `<`.
    pub isa_arrow: Option<bool>,
    /// Per-edge line color override.
    pub edge_color: Option<String>,
}

/// A positioned note annotation.
#[derive(Debug, Clone)]
pub struct ErdNoteLayout {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub lines: Vec<String>,
    pub connector: Option<(f64, f64, f64, f64)>,
}

/// A positioned ISA circle node (Java renders ISA as circle, not triangle).
#[derive(Debug, Clone)]
pub struct ErdIsaLayout {
    pub parent_id: String,
    pub kind_label: String,
    /// Center of the ISA circle node.
    pub center: (f64, f64),
    /// Full center node ID (e.g., "CHILD/d TODDLER, PRIMARY_AGE, TEEN /center").
    pub center_id: String,
    /// Radius of the ISA circle (Java: 12.5).
    pub radius: f64,
    /// Edge from parent entity to ISA center.
    pub parent_edge_path: Option<String>,
    /// Link uid for the parent→center edge.
    pub parent_edge_uid: usize,
    /// Edges from ISA center to each child entity.
    pub child_edges: Vec<ErdIsaChildEdge>,
    /// Whether the parent→center edge is double-stroke (from `=>=`).
    pub is_double: bool,
    /// Source declaration order (shared counter with entities and relationships).
    pub source_order: usize,
    /// Per-ISA background color override.
    pub bg_color: Option<String>,
    /// Per-ISA border/line color override.
    pub line_color: Option<String>,
}

/// An edge from an ISA center to a child entity.
#[derive(Debug, Clone)]
pub struct ErdIsaChildEdge {
    pub child_id: String,
    pub raw_path_d: Option<String>,
    /// Link uid for this center→child edge.
    pub link_uid: usize,
}

// ---------------------------------------------------------------------------
// Constants – tuned to match Java PlantUML reference output
// ---------------------------------------------------------------------------

const FONT_SIZE: f64 = 14.0;
const ENTITY_PADDING: f64 = 10.0;
const ENTITY_MIN_WIDTH: f64 = 0.0;
/// Entity rectangle height = title line_height + 2*MARGIN(5) + 2*MARGIN_LINE(5).
/// Java EntityImageChenEntity.calculateDimension: dim.delta(MARGIN*2 + 2*MARGIN_LINE) where
/// dim is the title TextBlock dimension. For SansSerif 14pt the line height is
/// 16.296875 (DejaVu Sans hhea ascent+descent / upem * size), giving 36.296875.
/// We must keep full precision because graphviz `pixelToInches` formats with 6 decimals,
/// and rounding to 4 decimals here desyncs the DOT input from Java's pipeline.
const ENTITY_HEIGHT: f64 = 36.296875;
/// Java MARGIN constant from IEntityImage (used for diamond calculation)
const JAVA_ENTITY_MARGIN: f64 = 5.0;
const MARGIN: f64 = 7.0;
/// Java ISA circle node diameter = 25 (radius 12.5).
const ISA_CIRCLE_DIAMETER: f64 = 25.0;
const NOTE_PADDING: f64 = 10.0;
const NOTE_LINE_HEIGHT: f64 = 16.0;
const NOTE_GAP: f64 = 16.0;

// ---------------------------------------------------------------------------
// Text measurement
// ---------------------------------------------------------------------------

fn text_width(text: &str) -> f64 {
    font_metrics::text_width(text, "SansSerif", FONT_SIZE, false, false)
}

fn entity_width(name: &str) -> f64 {
    (text_width(name) + 2.0 * ENTITY_PADDING).max(ENTITY_MIN_WIDTH)
}

/// Compute relationship diamond dimensions matching Java's ChenRelationship formula:
/// diagonal = (dimTitle.width + 2 * dimTitle.height) / sqrt(5) + 2 * MARGIN
/// totalWidth = diagonal * sqrt(5)
/// totalHeight = diagonal * sqrt(5) / 2
fn relationship_diamond_size(name: &str) -> (f64, f64) {
    let tw = text_width(name);
    let th = font_metrics::line_height("SansSerif", FONT_SIZE, false, false);
    let diagonal = (tw + 2.0 * th) / 5.0_f64.sqrt() + 2.0 * JAVA_ENTITY_MARGIN;
    let total_w = diagonal * 5.0_f64.sqrt();
    let total_h = diagonal * 5.0_f64.sqrt() / 2.0;
    (total_w, total_h)
}

/// Java MARGIN constant from EntityImageChenAttribute (used in bigger(6))
const ATTR_ELLIPSE_MARGIN: f64 = 6.0;

/// Compute attribute ellipse (width, height) matching Java's TextBlockInEllipse.
///
/// Java flow:
///   1. text dimensions (tw, th) via StringBounder
///   2. alpha = clamp(th/tw, 0.2, 0.8)
///   3. Footprint collects 4 text-corner points
///   4. Y-transform: y /= alpha to make isotropic
///   5. Smallest enclosing circle → radius r
///   6. Ellipse: width = 2r, height = 2r*alpha
///   7. .bigger(MARGIN=6): width += 6, height += 6
///
/// For single-line text, the 4 points form a rectangle (tw × th/alpha) after
/// y-transform, so SEC radius = sqrt(tw² + (th/alpha)²) / 2.
/// When alpha = th/tw (not clamped), this simplifies to radius = tw*sqrt(2)/2.
fn attr_ellipse_size(label: &str) -> (f64, f64) {
    let tw = text_width(label);
    let th = font_metrics::line_height("SansSerif", FONT_SIZE, false, false);

    if tw < 0.001 {
        // Degenerate: empty label
        return (ATTR_ELLIPSE_MARGIN * 2.0, ATTR_ELLIPSE_MARGIN * 2.0);
    }

    let alpha_raw = th / tw;
    let alpha = alpha_raw.clamp(0.2, 0.8);

    // After y-transform, the text rectangle becomes tw × (th/alpha).
    // SEC of a rectangle with sides W × H has radius = sqrt(W² + H²) / 2.
    let h_transformed = th / alpha;
    let radius = (tw * tw + h_transformed * h_transformed).sqrt() / 2.0;

    let ellipse_w = 2.0 * radius + ATTR_ELLIPSE_MARGIN;
    let ellipse_h = 2.0 * radius * alpha + ATTR_ELLIPSE_MARGIN;
    (ellipse_w, ellipse_h)
}

// ---------------------------------------------------------------------------
// Core layout
// ---------------------------------------------------------------------------

/// Metadata for a flattened attribute node that will be sent through graphviz.
struct AttrMeta {
    /// Unique graphviz node ID (e.g. "DIRECTOR/Number")
    id: String,
    /// Display label
    label: String,
    /// Parent node graphviz ID (entity, relationship, or parent attribute)
    parent_id: String,
    is_key: bool,
    is_derived: bool,
    is_multi: bool,
    has_type: bool,
    type_label: Option<String>,
    /// Graphviz node size: (width, height) of the bounding ellipse
    size: (f64, f64),
    /// IDs of child attributes (for nested attribute tree reconstruction)
    child_ids: Vec<String>,
    /// Per-attribute color spec (raw, e.g. "#lime;line:orange")
    color: Option<String>,
    /// 0-indexed source line number of the attribute declaration.
    attr_source_line: usize,
}

/// Recursively flatten attributes into a list of AttrMeta, creating graphviz
/// node IDs matching Java's convention: "owner_id/attr_name".
fn flatten_attributes(attrs: &[ErdAttribute], owner_id: &str, out: &mut Vec<AttrMeta>) {
    for attr in attrs {
        let display = attr.display_name.as_deref().unwrap_or(&attr.name);
        let full_label = if let Some(ref t) = attr.attr_type {
            format!("{} : {}", display, t)
        } else {
            display.to_string()
        };
        // Java uses internal name (+ optional type) for attribute IDs in links,
        // NOT the display name. E.g. `"No." as Number` → ID uses "Number".
        let id_name = if let Some(ref t) = attr.attr_type {
            format!("{} : {}", attr.name, t)
        } else {
            attr.name.clone()
        };
        let attr_id = format!("{}/{}", owner_id, id_name);
        let size = attr_ellipse_size(&full_label);

        let child_ids: Vec<String> = attr
            .children
            .iter()
            .map(|c| format!("{}/{}", attr_id, c.name))
            .collect();

        out.push(AttrMeta {
            id: attr_id.clone(),
            label: full_label,
            parent_id: owner_id.to_string(),
            is_key: attr.is_key,
            is_derived: attr.is_derived,
            is_multi: attr.is_multi,
            has_type: attr.attr_type.is_some(),
            type_label: attr.attr_type.clone(),
            size,
            child_ids: child_ids.clone(),
            color: attr.color.clone(),
            attr_source_line: attr.source_line,
        });

        // Recurse for nested attributes
        if !attr.children.is_empty() {
            flatten_attributes(&attr.children, &attr_id, out);
        }
    }
}

/// Perform the complete layout of an ERD using the svek/graphviz pipeline.
///
/// Java PlantUML routes ERD diagrams through the same graphviz DOT engine
/// as class diagrams. Entities become rect nodes, relationships become
/// diamond nodes, attributes become ellipse nodes, and all connections
/// (links + attribute-parent) become edges.
pub fn layout_erd(diagram: &ErdDiagram) -> Result<ErdLayout> {
    debug!(
        "layout_erd: {} entities, {} relationships, {} links, {} ISAs, direction={:?}",
        diagram.entities.len(),
        diagram.relationships.len(),
        diagram.links.len(),
        diagram.isas.len(),
        diagram.direction
    );

    let is_lr = diagram.direction == ErdDirection::LeftToRight;

    // Build name lookup: id -> display name

    // Entity index map for link metadata
    let mut entity_idx: HashMap<String, usize> = HashMap::new();
    for (i, e) in diagram.entities.iter().enumerate() {
        entity_idx.insert(e.id.clone(), i);
    }
    for (i, r) in diagram.relationships.iter().enumerate() {
        entity_idx.insert(r.id.clone(), diagram.entities.len() + i);
    }

    // ── Flatten all attributes into graphviz node metadata ──
    // Must follow source declaration order (entities and relationships interleaved)
    // so that edge indices align with Java's output.
    let mut attr_metas: Vec<AttrMeta> = Vec::new();
    {
        enum Item<'a> {
            E(&'a crate::model::erd::ErdEntity),
            R(&'a crate::model::erd::ErdRelationship),
        }
        let mut sorted: Vec<Item> = Vec::new();
        for e in &diagram.entities {
            sorted.push(Item::E(e));
        }
        for r in &diagram.relationships {
            sorted.push(Item::R(r));
        }
        sorted.sort_by_key(|item| match item {
            Item::E(e) => e.source_order,
            Item::R(r) => r.source_order,
        });
        for item in &sorted {
            match item {
                Item::E(e) => flatten_attributes(&e.attributes, &e.id, &mut attr_metas),
                Item::R(r) => flatten_attributes(&r.attributes, &r.id, &mut attr_metas),
            }
        }
    }

    // ── Build svek layout nodes ──
    // Java emits nodes in parse order: each entity/relationship followed by
    // its attributes. This ordering affects graphviz's horizontal layout.
    let rankdir = if is_lr {
        RankDir::LeftToRight
    } else {
        RankDir::TopToBottom
    };
    let mut layout_nodes: Vec<LayoutNode> = Vec::new();

    // Build an index: parent_id → list of AttrMeta for that parent
    let mut attrs_by_parent: HashMap<String, Vec<&AttrMeta>> = HashMap::new();
    for am in &attr_metas {
        attrs_by_parent
            .entry(am.parent_id.clone())
            .or_default()
            .push(am);
    }

    // Helper: add an attribute node and recursively its children
    fn add_attr_nodes(
        am: &AttrMeta,
        attrs_by_parent: &HashMap<String, Vec<&AttrMeta>>,
        layout_nodes: &mut Vec<LayoutNode>,
    ) {
        let (w, h) = am.size;
        layout_nodes.push(LayoutNode {
            id: am.id.clone(),
            label: am.label.clone(),
            width_pt: w,
            height_pt: h,
            shape: Some(ShapeType::Oval),
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
        // Recursively add children
        if let Some(children) = attrs_by_parent.get(&am.id) {
            for child in children {
                add_attr_nodes(child, attrs_by_parent, layout_nodes);
            }
        }
    }

    // Entities and relationships in source declaration order (interleaved).
    // Java emits them in parse order; this ordering affects graphviz placement.
    enum ErdItem<'a> {
        Entity(&'a crate::model::erd::ErdEntity),
        Relationship(&'a crate::model::erd::ErdRelationship),
    }
    let mut items: Vec<ErdItem> = Vec::new();
    for e in &diagram.entities {
        items.push(ErdItem::Entity(e));
    }
    for r in &diagram.relationships {
        items.push(ErdItem::Relationship(r));
    }
    items.sort_by_key(|item| match item {
        ErdItem::Entity(e) => e.source_order,
        ErdItem::Relationship(r) => r.source_order,
    });

    for item in &items {
        match item {
            ErdItem::Entity(e) => {
                let w = entity_width(&e.name);
                layout_nodes.push(LayoutNode {
                    id: e.id.clone(),
                    label: e.name.clone(),
                    width_pt: w,
                    height_pt: ENTITY_HEIGHT,
                    shape: Some(ShapeType::Rectangle),
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
                });
                if let Some(attrs) = attrs_by_parent.get(&e.id) {
                    for am in attrs {
                        add_attr_nodes(am, &attrs_by_parent, &mut layout_nodes);
                    }
                }
            }
            ErdItem::Relationship(r) => {
                let (w, h) = relationship_diamond_size(&r.name);
                layout_nodes.push(LayoutNode {
                    id: r.id.clone(),
                    label: r.name.clone(),
                    width_pt: w,
                    height_pt: h,
                    shape: Some(ShapeType::Diamond),
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
                });
                if let Some(attrs) = attrs_by_parent.get(&r.id) {
                    for am in attrs {
                        add_attr_nodes(am, &attrs_by_parent, &mut layout_nodes);
                    }
                }
            }
        }
    }

    // ISA center nodes → circle nodes
    // Java convention: id = "{parent}/{kind} {child1}, {child2}, ... /center"
    struct IsaNodeMeta {
        center_id: String,
        parent_id: String,
        child_ids: Vec<String>,
        is_double: bool,
        source_order: usize,
        color: Option<String>,
    }
    let mut isa_node_metas: Vec<IsaNodeMeta> = Vec::new();
    for isa in &diagram.isas {
        let kind_str = match isa.kind {
            crate::model::erd::IsaKind::Disjoint => "d",
            crate::model::erd::IsaKind::Union => "U",
        };
        let children_str = isa.children.join(", ");
        let center_id = format!("{}/{} {} /center", isa.parent, kind_str, children_str);
        layout_nodes.push(LayoutNode {
            id: center_id.clone(),
            label: kind_str.to_string(),
            width_pt: ISA_CIRCLE_DIAMETER,
            height_pt: ISA_CIRCLE_DIAMETER,
            shape: Some(ShapeType::Oval),
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
        isa_node_metas.push(IsaNodeMeta {
            center_id,
            parent_id: isa.parent.clone(),
            child_ids: isa.children.clone(),
            is_double: isa.is_double,
            source_order: isa.source_order,
            color: isa.color.clone(),
        });
    }

    // ── Build svek layout edges ──
    let mut layout_edges: Vec<LayoutEdge> = Vec::new();

    // Link edges (entity↔relationship) with cardinality labels
    // ISA arrow links (->- and -<-) have no label in Java (Display.NULL).
    let label_dims: Vec<Option<(f64, f64)>> = diagram
        .links
        .iter()
        .map(|link| {
            if link.isa_arrow.is_some() {
                None // ISA arrow links have no label
            } else {
                let tw =
                    font_metrics::text_width(&link.cardinality, "SansSerif", 11.0, false, false);
                let th = font_metrics::line_height("SansSerif", 11.0, false, false);
                let dim_w = (tw + 2.0).floor();
                let dim_h = (th + 2.0).floor();
                Some((dim_w, dim_h))
            }
        })
        .collect();

    let num_link_edges = diagram.links.len();
    for (i, link) in diagram.links.iter().enumerate() {
        let (label, label_dimension) = if let Some((lw, lh)) = label_dims[i] {
            (Some(link.cardinality.clone()), Some((lw, lh)))
        } else {
            (None, None)
        };
        layout_edges.push(LayoutEdge {
            from: link.from.clone(),
            to: link.to.clone(),
            label,
            label_dimension,
            tail_label: None,
            tail_label_dimension: None,
            tail_label_boxed: false,
            head_label: None,
            head_label_dimension: None,
            head_label_boxed: false,
            tail_decoration: crate::svek::edge::LinkDecoration::None,
            head_decoration: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Normal,
            minlen: 2,
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
    }

    // Attribute→parent edges (Java: Link with length=2, DOT minlen=length-1=1)
    for am in &attr_metas {
        layout_edges.push(LayoutEdge {
            from: am.id.clone(),
            to: am.parent_id.clone(),
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
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
    }

    // ISA edges: parent→center and center→child for each ISA
    let num_pre_isa_edges = layout_edges.len();
    for isa_meta in &isa_node_metas {
        // Parent → ISA center edge (Java: LinkArg length=2 → DOT minlen=1)
        layout_edges.push(LayoutEdge {
            from: isa_meta.parent_id.clone(),
            to: isa_meta.center_id.clone(),
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
            invisible: false,
            is_opale: false,
            no_constraint: false,
        });
        // ISA center → each child (Java: LinkArg length=3 → DOT minlen=2)
        for child_id in &isa_meta.child_ids {
            layout_edges.push(LayoutEdge {
                from: isa_meta.center_id.clone(),
                to: child_id.clone(),
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
                minlen: 2,
                invisible: false,
                is_opale: false,
                no_constraint: false,
            });
        }
    }

    // Build svek uid map: node ID → uid, matching Java's IEntity.getUid().
    // In Java, nodes and edges interleave in the DOT model, each consuming
    // one uid slot. UIDs start at 2 (first 2 reserved for diagram metadata).
    // Order: for each entity/relationship in source order, add the node,
    // then its attributes (each attr node + attr edge pair), then link edges.
    let (svek_node_uids, link_uids, link_source_lines, isa_edge_uids) = {
        let mut uid_map: HashMap<String, usize> = HashMap::new();
        let mut lnk_uids: HashMap<usize, usize> = HashMap::new();
        let mut lnk_source_lines: HashMap<usize, usize> = HashMap::new();
        let mut isa_edge_uids: HashMap<usize, (usize, Vec<usize>)> = HashMap::new();
        let mut uid = 2usize;

        // Build attr_metas index by parent_id for recursive uid assignment
        let mut ametas_by_parent: HashMap<&str, Vec<&AttrMeta>> = HashMap::new();
        for am in &attr_metas {
            ametas_by_parent
                .entry(am.parent_id.as_str())
                .or_default()
                .push(am);
        }

        // Helper to recursively assign uids to an attribute and its children
        fn assign_attr_uids(
            am: &AttrMeta,
            ametas_by_parent: &HashMap<&str, Vec<&AttrMeta>>,
            uid_map: &mut HashMap<String, usize>,
            uid: &mut usize,
        ) {
            uid_map.insert(am.id.clone(), *uid);
            *uid += 1; // node uid
            *uid += 1; // edge uid (attr→parent)
            if let Some(children) = ametas_by_parent.get(am.id.as_str()) {
                for child in children {
                    assign_attr_uids(child, ametas_by_parent, uid_map, uid);
                }
            }
        }

        // Merge entities, relationships, links, and ISAs in source order
        enum UidItem<'a> {
            Entity(&'a crate::model::erd::ErdEntity),
            Relationship(&'a crate::model::erd::ErdRelationship),
            Link(&'a ErdLink),
            Isa(&'a crate::model::erd::ErdIsa),
        }
        let mut uid_items: Vec<(usize, u8, UidItem)> = Vec::new();
        for e in &diagram.entities {
            uid_items.push((e.source_order, 0, UidItem::Entity(e)));
        }
        for r in &diagram.relationships {
            uid_items.push((r.source_order, 0, UidItem::Relationship(r)));
        }
        for l in &diagram.links {
            uid_items.push((l.source_order, 1, UidItem::Link(l)));
        }
        for isa in &diagram.isas {
            uid_items.push((isa.source_order, 1, UidItem::Isa(isa)));
        }
        uid_items.sort_by_key(|(order, prio, _)| (*order, *prio));

        for (_, _, item) in &uid_items {
            match item {
                UidItem::Entity(e) => {
                    uid_map.insert(e.id.clone(), uid);
                    uid += 1;
                    // Assign uids to direct attributes
                    if let Some(attrs) = ametas_by_parent.get(e.id.as_str()) {
                        for am in attrs {
                            assign_attr_uids(am, &ametas_by_parent, &mut uid_map, &mut uid);
                        }
                    }
                }
                UidItem::Relationship(r) => {
                    uid_map.insert(r.id.clone(), uid);
                    uid += 1;
                    if let Some(attrs) = ametas_by_parent.get(r.id.as_str()) {
                        for am in attrs {
                            assign_attr_uids(am, &ametas_by_parent, &mut uid_map, &mut uid);
                        }
                    }
                }
                UidItem::Link(l) => {
                    lnk_uids.insert(l.source_order, uid);
                    lnk_source_lines.insert(l.source_order, l.source_line);
                    uid += 1; // link edge consumes one uid
                }
                UidItem::Isa(isa) => {
                    let kind_str = match isa.kind {
                        crate::model::erd::IsaKind::Disjoint => "d",
                        crate::model::erd::IsaKind::Union => "U",
                    };
                    let children_str = isa.children.join(", ");
                    let center_id = format!("{}/{} {} /center", isa.parent, kind_str, children_str);
                    uid_map.insert(center_id, uid);
                    uid += 1;
                    // parent→center edge uid
                    let parent_edge_uid = uid;
                    uid += 1;
                    // center→child edge uids
                    let mut child_uids = Vec::new();
                    for _ in &isa.children {
                        child_uids.push(uid);
                        uid += 1;
                    }
                    isa_edge_uids.insert(isa.source_order, (parent_edge_uid, child_uids));
                }
            }
        }
        (uid_map, lnk_uids, lnk_source_lines, isa_edge_uids)
    };

    let graph = LayoutGraph {
        nodes: layout_nodes,
        edges: layout_edges,
        clusters: vec![],
        rankdir,
        is_activity: false,
        ranksep_override: None,
        nodesep_override: None,
        use_simplier_dot_link_strategy: false,
        arrow_font_size: None,
    };

    let gl = graphviz::layout_with_svek(&graph)
        .map_err(|e| crate::error::Error::Layout(format!("ERD svek layout: {e}")))?;

    // Render offsets.
    // When all nodes are rects (lf_rect_correction=true), the LF min has an
    // extra -1 that inflates render_offset by 1. Subtract it back to align
    // with Java's final coordinate space.
    // When attributes (ellipses) or diamonds are present, the topmost node
    // may use different LF corrections (no -1 for ellipses, or polygon hack
    // for diamonds), so render_offset is already correct.
    let has_non_rect_nodes = !attr_metas.is_empty() || !diagram.relationships.is_empty();
    let render_dx = gl.render_offset.0;
    let render_dy = if has_non_rect_nodes {
        gl.render_offset.1
    } else {
        gl.render_offset.1 - 1.0
    };
    let _render_dy_label = gl.render_offset.1;

    debug!(
        "layout_erd svek: render_offset=({:.2},{:.2}), move_delta=({:.2},{:.2}), lf_span=({:.2},{:.2}), normalize_offset=({:.2},{:.2})",
        gl.render_offset.0, gl.render_offset.1,
        gl.move_delta.0, gl.move_delta.1,
        gl.lf_span.0, gl.lf_span.1,
        gl.normalize_offset.0, gl.normalize_offset.1,
    );

    // Collect all node positions: (top_left_x, top_left_y, width, height)
    // Use min_x/min_y from graphviz (not cx-width/2) to handle ellipse nodes
    // where graphviz rounds rx/ry, causing cx-width/2 to differ from min_x.
    let mut positions: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();
    for nl in &gl.nodes {
        let x = nl.min_x + render_dx;
        let y = nl.min_y + render_dy;
        positions.insert(nl.id.clone(), (x, y, nl.width, nl.height));
    }

    // Build entity node layouts
    let entity_nodes: Vec<ErdNodeLayout> = diagram
        .entities
        .iter()
        .filter_map(|e| {
            let (x, y, w, h) = positions.get(&e.id).copied()?;
            let (bg_color, line_color) = parse_erd_color_spec(e.color.as_deref());
            Some(ErdNodeLayout {
                id: e.id.clone(),
                label: e.name.clone(),
                x,
                y,
                width: w,
                height: h,
                is_weak: e.is_weak,
                is_identifying: false,
                is_relationship: false,
                source_order: e.source_order,
                bg_color,
                line_color,
            })
        })
        .collect();

    // Build relationship node layouts
    let relationship_nodes: Vec<ErdNodeLayout> = diagram
        .relationships
        .iter()
        .filter_map(|r| {
            let (x, y, w, h) = positions.get(&r.id).copied()?;
            let (bg_color, line_color) = parse_erd_color_spec(r.color.as_deref());
            Some(ErdNodeLayout {
                id: r.id.clone(),
                label: r.name.clone(),
                x,
                y,
                width: w,
                height: h,
                is_weak: false,
                is_identifying: r.is_identifying,
                is_relationship: true,
                source_order: r.source_order,
                bg_color,
                line_color,
            })
        })
        .collect();

    // ── Build attribute layouts from graphviz positions ──
    // First pass: create flat ErdAttrLayout for each attribute
    let mut attr_layout_map: HashMap<String, ErdAttrLayout> = HashMap::new();
    for am in &attr_metas {
        if let Some(&(x, y, w, h)) = positions.get(&am.id) {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let rx = w / 2.0;
            let ry = h / 2.0;
            let (bg_color, line_color) = parse_erd_color_spec(am.color.as_deref());
            attr_layout_map.insert(
                am.id.clone(),
                ErdAttrLayout {
                    id: am.id.clone(),
                    label: am.label.clone(),
                    parent: am.parent_id.clone(),
                    x: cx,
                    y: cy,
                    rx,
                    ry,
                    is_key: am.is_key,
                    is_derived: am.is_derived,
                    is_multi: am.is_multi,
                    has_type: am.has_type,
                    type_label: am.type_label.clone(),
                    children: Vec::new(),
                    bg_color,
                    line_color,
                },
            );
        }
    }

    // Second pass: attach child layouts to parent attributes
    // Process in reverse so children are populated before being moved to parents
    for am in attr_metas.iter().rev() {
        for child_id in &am.child_ids {
            if let Some(child_layout) = attr_layout_map.remove(child_id) {
                if let Some(parent_layout) = attr_layout_map.get_mut(&am.id) {
                    parent_layout.children.push(child_layout);
                }
            }
        }
    }

    // Collect top-level attributes (those whose parent is an entity/relationship)
    let entity_and_rel_ids: std::collections::HashSet<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.clone())
        .chain(diagram.relationships.iter().map(|r| r.id.clone()))
        .collect();
    let attribute_nodes: Vec<ErdAttrLayout> = attr_metas
        .iter()
        .filter(|am| entity_and_rel_ids.contains(&am.parent_id))
        .filter_map(|am| attr_layout_map.remove(&am.id))
        .collect();

    // ── Layout edges from svek results ──
    // Only the first `num_link_edges` svek edges correspond to diagram links.
    // The remaining edges are attribute→parent (rendered separately).
    let mut edges: Vec<ErdEdgeLayout> = Vec::new();
    for (li, link) in diagram.links.iter().enumerate() {
        let from_name = link.from.clone();
        let to_name = link.to.clone();
        let from_idx = entity_idx.get(&link.from).copied().unwrap_or(0);
        let to_idx = entity_idx.get(&link.to).copied().unwrap_or(0);

        let svek_edge = gl.edges.get(li);
        let raw_path_d = svek_edge
            .and_then(|e| e.raw_path_d.as_ref())
            .map(|d| shift_svg_path(d, render_dx, render_dy));
        // Java: label is drawn at label_polygon_min_xy + moveDelta.
        // Our label_xy is from svek solve (pre-moveDelta, pre-normalize), so we
        // need to apply moveDelta and the render-vs-normalize offset correction
        // (same as class diagram labels: +moveDelta -normalizeOffset +renderOffset).
        let label_xy = svek_edge.and_then(|e| {
            let (lx, ly) = e.label_xy?;
            Some((
                lx + gl.move_delta.0 - gl.normalize_offset.0 + gl.render_offset.0,
                ly + gl.move_delta.1 - gl.normalize_offset.1 + gl.render_offset.1,
            ))
        });

        let (from_point, to_point) =
            if let (Some(fp), Some(tp)) = (positions.get(&link.from), positions.get(&link.to)) {
                let (fx, fy, fw, fh) = *fp;
                let (tx, ty, tw, th) = *tp;
                let fc = (fx + fw / 2.0, fy + fh / 2.0);
                let tc = (tx + tw / 2.0, ty + th / 2.0);
                (
                    clip_to_rect(fc.0, fc.1, fw, fh, tc.0, tc.1),
                    clip_to_rect(tc.0, tc.1, tw, th, fc.0, fc.1),
                )
            } else {
                ((0.0, 0.0), (0.0, 0.0))
            };

        edges.push(ErdEdgeLayout {
            from_id: link.from.clone(),
            to_id: link.to.clone(),
            from_name,
            to_name,
            from_point,
            to_point,
            label: link.cardinality.clone(),
            is_double: link.is_double,
            source_line: 0,
            entity_idx_from: from_idx,
            entity_idx_to: to_idx,
            raw_path_d,
            label_xy,
            source_order: link.source_order,
            isa_arrow: link.isa_arrow,
            edge_color: {
                // For links, the color spec sets the stroke color.
                // `#lime` → stroke=lime; `#lime;line:green` → stroke=green
                let (bg, lc) = parse_erd_color_spec(link.color.as_deref());
                lc.or(bg) // prefer line_color, fall back to bg_color
            },
        });
    }

    // Build ISA layouts from graphviz results
    let mut isa_layouts: Vec<ErdIsaLayout> = Vec::new();
    let mut isa_edge_idx = num_pre_isa_edges;
    for isa_meta in &isa_node_metas {
        let (cx, cy) = if let Some(&(x, y, w, h)) = positions.get(&isa_meta.center_id) {
            (x + w / 2.0, y + h / 2.0)
        } else {
            log::warn!(
                "ISA center node '{}' not found in layout",
                isa_meta.center_id
            );
            continue;
        };

        let kind_label = if isa_meta.center_id.contains("/d ") {
            "d".to_string()
        } else {
            "U".to_string()
        };

        // Parent→center edge path
        let parent_edge_path = gl
            .edges
            .get(isa_edge_idx)
            .and_then(|e| e.raw_path_d.as_ref())
            .map(|d| shift_svg_path(d, render_dx, render_dy));
        isa_edge_idx += 1;

        // ISA edge UIDs
        let (parent_edge_uid, child_edge_uids) = isa_edge_uids
            .get(&isa_meta.source_order)
            .cloned()
            .unwrap_or_else(|| (0, vec![0; isa_meta.child_ids.len()]));

        // Center→child edge paths
        let mut child_edges = Vec::new();
        for (ci, child_id) in isa_meta.child_ids.iter().enumerate() {
            let child_path = gl
                .edges
                .get(isa_edge_idx)
                .and_then(|e| e.raw_path_d.as_ref())
                .map(|d| shift_svg_path(d, render_dx, render_dy));
            child_edges.push(ErdIsaChildEdge {
                child_id: child_id.clone(),
                raw_path_d: child_path,
                link_uid: child_edge_uids.get(ci).copied().unwrap_or(0),
            });
            isa_edge_idx += 1;
        }

        let (isa_bg, isa_line) = parse_erd_color_spec(isa_meta.color.as_deref());
        isa_layouts.push(ErdIsaLayout {
            parent_id: isa_meta.parent_id.clone(),
            kind_label,
            center: (cx, cy),
            center_id: isa_meta.center_id.clone(),
            radius: ISA_CIRCLE_DIAMETER / 2.0,
            parent_edge_path,
            parent_edge_uid,
            child_edges,
            is_double: isa_meta.is_double,
            source_order: isa_meta.source_order,
            bg_color: isa_bg,
            line_color: isa_line,
        });
    }

    // Viewport: Java ImageBuilder.getFinalDimension() — shifted LF max + 1.
    let is_degenerated = entity_nodes.len() + relationship_nodes.len() <= 1 && edges.is_empty();
    let (raw_body_w, raw_body_h) =
        if is_degenerated && (entity_nodes.len() + relationship_nodes.len()) == 1 {
            const DEGENERATED_DELTA: f64 = 7.0;
            let n = entity_nodes.first().or(relationship_nodes.first()).unwrap();
            (
                n.width + DEGENERATED_DELTA * 2.0 + 1.0,
                n.height + DEGENERATED_DELTA * 2.0 + 1.0,
            )
        } else {
            // Java moveDelta = (6 - lf_min), so shifted_max = lf_span + 6.
            const SVEK_MOVE_DELTA: f64 = 6.0;
            let shifted_max_x = gl.lf_span.0 + SVEK_MOVE_DELTA;
            let shifted_max_y = gl.lf_span.1 + SVEK_MOVE_DELTA;
            (shifted_max_x + 1.0, shifted_max_y + 1.0)
        };

    let mut max_right = raw_body_w;
    let mut max_bottom = raw_body_h;

    // ISA nodes are now in graphviz, so their positions are included in lf_span.
    // No manual ISA viewport expansion needed.

    let notes = layout_notes(&diagram.notes, &positions, max_right, max_bottom);

    for note in &notes {
        let nr = note.x + note.width - render_dx + ViewportConfig::COMPONENT.margin_right;
        let nb = note.y + note.height - render_dy + ViewportConfig::COMPONENT.margin_bottom;
        max_right = max_right.max(nr);
        max_bottom = max_bottom.max(nb);
    }

    let width = max_right + ViewportConfig::COMPONENT.margin_right;
    let height = max_bottom + ViewportConfig::COMPONENT.margin_bottom;

    debug!(
        "layout_erd done: {:.0}x{:.0} (lf_span={:.1}x{:.1}), {} ents, {} rels, {} attrs, {} edges, {} ISAs, {} notes",
        width, height, gl.lf_span.0, gl.lf_span.1,
        entity_nodes.len(), relationship_nodes.len(), attribute_nodes.len(),
        edges.len(), isa_layouts.len(), notes.len()
    );

    // Build lookup: entity/relationship ID → source_order
    let mut id_to_source_order: HashMap<String, usize> = HashMap::new();
    for e in &diagram.entities {
        id_to_source_order.insert(e.id.clone(), e.source_order);
    }
    for r in &diagram.relationships {
        id_to_source_order.insert(r.id.clone(), r.source_order);
    }

    // Extract attribute→parent edge paths from svek results.
    // These are edges at indices [num_link_edges..num_pre_isa_edges] in the graphviz output.
    let attr_edges: Vec<ErdAttrEdge> = (num_link_edges..num_pre_isa_edges)
        .enumerate()
        .map(|(j, i)| {
            let raw_path_d = gl
                .edges
                .get(i)
                .and_then(|e| e.raw_path_d.as_ref())
                .map(|d| shift_svg_path(d, render_dx, render_dy));
            let from_name = attr_metas
                .get(j)
                .map(|am| am.id.clone())
                .unwrap_or_default();
            let to_name = attr_metas
                .get(j)
                .map(|am| am.parent_id.clone())
                .unwrap_or_default();
            // Find the source_order of the root parent entity/relationship.
            // For nested attrs, walk up: attr parent_id → ... → entity/relationship.
            let parent_source_order = attr_metas
                .get(j)
                .and_then(|am| {
                    // Walk up the parent chain to find the entity/relationship
                    let mut pid = &am.parent_id;
                    loop {
                        if let Some(&so) = id_to_source_order.get(pid.as_str()) {
                            return Some(so);
                        }
                        // Try to find this pid in attr_metas to get its parent
                        if let Some(parent_am) = attr_metas.iter().find(|a| a.id == *pid) {
                            pid = &parent_am.parent_id;
                        } else {
                            return None;
                        }
                    }
                })
                .unwrap_or(0);
            let edge_color = attr_metas.get(j).and_then(|am| {
                let (_, lc) = parse_erd_color_spec(am.color.as_deref());
                lc
            });
            let attr_source_line = attr_metas.get(j).map(|am| am.attr_source_line).unwrap_or(0);
            ErdAttrEdge {
                raw_path_d,
                from_name,
                to_name,
                parent_source_order,
                edge_color,
                attr_source_line,
            }
        })
        .collect();

    Ok(ErdLayout {
        entity_nodes,
        relationship_nodes,
        attribute_nodes,
        edges,
        attr_edges,
        isa_layouts,
        notes,
        width,
        height,
        svek_node_uids,
        link_uids,
        link_source_lines,
    })
}

/// Parse an ERD color spec like `#lightblue;line:blue` into (bg_color, line_color).
fn parse_erd_color_spec(spec: Option<&str>) -> (Option<String>, Option<String>) {
    let spec = match spec {
        Some(s) => s.trim(),
        None => return (None, None),
    };
    let spec = spec.strip_prefix('#').unwrap_or(spec);
    let mut bg = None;
    let mut line = None;
    for part in spec.split(';') {
        let part = part.trim();
        if let Some(lc) = part.strip_prefix("line:") {
            line = Some(resolve_color_name(lc.trim()));
        } else if !part.is_empty() {
            bg = Some(resolve_color_name(part));
        }
    }
    (bg, line)
}

/// Resolve a color name to its hex value.
fn resolve_color_name(name: &str) -> String {
    // If already a hex color, return as-is
    if name.starts_with('#') {
        return name.to_uppercase();
    }
    // Common HTML color names
    match name.to_lowercase().as_str() {
        "lightblue" => "#ADD8E6".to_string(),
        "blue" => "#0000FF".to_string(),
        "red" => "#FF0000".to_string(),
        "green" => "#008000".to_string(),
        "lime" => "#00FF00".to_string(),
        "orange" => "#FFA500".to_string(),
        "pink" => "#FFC0CB".to_string(),
        "purple" => "#800080".to_string(),
        "yellow" => "#FFFF00".to_string(),
        "white" => "#FFFFFF".to_string(),
        "black" => "#000000".to_string(),
        "gray" | "grey" => "#808080".to_string(),
        "navy" => "#000080".to_string(),
        _ => format!("#{}", name), // assume hex without #
    }
}

/// Shift all numeric coordinates in an SVG path d-string by (dx, dy).
fn shift_svg_path(d: &str, dx: f64, dy: f64) -> String {
    use crate::render::svg::fmt_coord;
    let mut result = String::with_capacity(d.len() + 32);
    let mut chars = d.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c == 'M' || c == 'C' || c == 'L' || c == ' ' {
            result.push(c);
            chars.next();
            continue;
        }
        if c == '-' || c == '.' || c.is_ascii_digit() {
            let mut num_str = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '-' || nc == '.' || nc.is_ascii_digit() {
                    num_str.push(nc);
                    chars.next();
                } else {
                    break;
                }
            }
            let x_val: f64 = num_str.parse().unwrap_or(0.0);
            result.push_str(&fmt_coord(x_val + dx));
            if let Some(&',') = chars.peek() {
                result.push(',');
                chars.next();
            }
            let mut num_str = String::new();
            while let Some(&nc) = chars.peek() {
                if nc == '-' || nc == '.' || nc.is_ascii_digit() {
                    num_str.push(nc);
                    chars.next();
                } else {
                    break;
                }
            }
            let y_val: f64 = num_str.parse().unwrap_or(0.0);
            result.push_str(&fmt_coord(y_val + dy));
        } else {
            result.push(c);
            chars.next();
        }
    }
    result
}

/// Clip a line from (cx, cy) toward (target_x, target_y) to the rectangle.
fn clip_to_rect(cx: f64, cy: f64, w: f64, h: f64, target_x: f64, target_y: f64) -> (f64, f64) {
    let dx = target_x - cx;
    let dy = target_y - cy;

    if dx.abs() < 0.001 && dy.abs() < 0.001 {
        return (cx, cy);
    }

    let half_w = w / 2.0;
    let half_h = h / 2.0;
    let mut t = f64::MAX;

    if dx.abs() > 0.001 {
        let tx = if dx > 0.0 { half_w / dx } else { -half_w / dx };
        if tx > 0.0 && tx < t {
            t = tx;
        }
    }
    if dy.abs() > 0.001 {
        let ty = if dy > 0.0 { half_h / dy } else { -half_h / dy };
        if ty > 0.0 && ty < t {
            t = ty;
        }
    }

    if t == f64::MAX {
        (cx, cy)
    } else {
        (cx + dx * t, cy + dy * t)
    }
}

// ---------------------------------------------------------------------------
// Note layout
// ---------------------------------------------------------------------------

fn estimate_note_size(text: &str) -> (f64, f64, Vec<String>) {
    let lines: Vec<String> = text.lines().map(std::string::ToString::to_string).collect();
    let line_refs: Vec<&str> = if lines.is_empty() {
        vec![""]
    } else {
        lines.iter().map(String::as_str).collect()
    };
    let max_width = line_refs
        .iter()
        .map(|line| text_width(line))
        .fold(0.0_f64, f64::max);
    let width = max_width + NOTE_PADDING * 2.0;
    let height = line_refs.len() as f64 * NOTE_LINE_HEIGHT + NOTE_PADDING * 2.0;
    let lines = if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    };
    (width.max(80.0), height.max(36.0), lines)
}

fn layout_notes(
    notes: &[crate::model::erd::ErdNote],
    positions: &HashMap<String, (f64, f64, f64, f64)>,
    base_max_x: f64,
    base_max_y: f64,
) -> Vec<ErdNoteLayout> {
    let mut result = Vec::new();
    let mut floating_y = MARGIN;

    for note in notes {
        let (width, height, lines) = estimate_note_size(&note.text);

        let (x, y, connector) = if let Some(target) = note.target.as_ref() {
            if let Some(&(tx, ty, tw, th)) = positions.get(target) {
                match note.position.as_str() {
                    "left" => (
                        tx - width - NOTE_GAP,
                        ty,
                        Some((tx - NOTE_GAP, ty + th / 2.0, tx, ty + th / 2.0)),
                    ),
                    "top" => (
                        tx + (tw - width) / 2.0,
                        ty - height - NOTE_GAP,
                        Some((tx + tw / 2.0, ty - NOTE_GAP, tx + tw / 2.0, ty)),
                    ),
                    "bottom" => (
                        tx + (tw - width) / 2.0,
                        ty + th + NOTE_GAP,
                        Some((tx + tw / 2.0, ty + th, tx + tw / 2.0, ty + th + NOTE_GAP)),
                    ),
                    _ => (
                        tx + tw + NOTE_GAP,
                        ty,
                        Some((tx + tw, ty + th / 2.0, tx + tw + NOTE_GAP, ty + th / 2.0)),
                    ),
                }
            } else {
                let x = base_max_x + NOTE_GAP;
                let y = floating_y;
                floating_y += height + NOTE_GAP;
                (x, y, None)
            }
        } else {
            let x = match note.position.as_str() {
                "left" => MARGIN,
                _ => base_max_x + NOTE_GAP,
            };
            let y = if note.position == "bottom" {
                base_max_y + NOTE_GAP + (floating_y - MARGIN)
            } else {
                floating_y
            };
            floating_y += height + NOTE_GAP;
            (x, y, None)
        };

        result.push(ErdNoteLayout {
            text: note.text.clone(),
            x,
            y,
            width,
            height,
            lines,
            connector,
        });
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::erd::*;

    fn empty_diagram() -> ErdDiagram {
        ErdDiagram {
            entities: vec![],
            relationships: vec![],
            links: vec![],
            isas: vec![],
            direction: ErdDirection::TopToBottom,
            notes: vec![],
        }
    }

    fn simple_entity(name: &str) -> ErdEntity {
        ErdEntity {
            id: name.to_string(),
            name: name.to_string(),
            attributes: vec![],
            is_weak: false,
            color: None,
            source_order: 0,
        }
    }

    fn simple_relationship(name: &str) -> ErdRelationship {
        ErdRelationship {
            id: name.to_string(),
            name: name.to_string(),
            attributes: vec![],
            is_identifying: false,
            color: None,
            source_order: 0,
        }
    }

    fn simple_attr(name: &str) -> ErdAttribute {
        ErdAttribute {
            name: name.to_string(),
            display_name: None,
            is_key: false,
            is_derived: false,
            is_multi: false,
            attr_type: None,
            children: vec![],
            color: None,
            source_line: 0,
        }
    }

    fn simple_link(from: &str, to: &str, card: &str) -> ErdLink {
        ErdLink {
            from: from.to_string(),
            to: to.to_string(),
            cardinality: card.to_string(),
            is_double: false,
            color: None,
            isa_arrow: None,
            source_order: 0,
            source_line: 0,
        }
    }

    #[test]
    fn test_empty_diagram() {
        let d = empty_diagram();
        let layout = layout_erd(&d).unwrap();
        assert!(layout.entity_nodes.is_empty());
        assert!(layout.relationship_nodes.is_empty());
        assert!(layout.attribute_nodes.is_empty());
        assert!(layout.edges.is_empty());
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
    }

    #[test]
    fn test_single_entity() {
        let d = ErdDiagram {
            entities: vec![simple_entity("MOVIE")],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.entity_nodes.len(), 1);
        let node = &layout.entity_nodes[0];
        assert_eq!(node.id, "MOVIE");
        assert!(node.width >= ENTITY_MIN_WIDTH);
        assert!((node.height - ENTITY_HEIGHT).abs() < 0.01);
    }

    #[test]
    fn test_entity_with_attributes() {
        let d = ErdDiagram {
            entities: vec![ErdEntity {
                id: "CUSTOMER".to_string(),
                name: "CUSTOMER".to_string(),
                attributes: vec![
                    ErdAttribute {
                        is_key: true,
                        ..simple_attr("Number")
                    },
                    simple_attr("Name"),
                ],
                is_weak: false,
                color: None,
                source_order: 0,
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.attribute_nodes.len(), 2);
        assert!(layout.attribute_nodes[0].is_key);
        assert_eq!(layout.attribute_nodes[0].parent, "CUSTOMER");
    }

    #[test]
    fn test_single_relationship() {
        let d = ErdDiagram {
            relationships: vec![simple_relationship("RENTED_TO")],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.relationship_nodes.len(), 1);
        assert_eq!(layout.relationship_nodes[0].id, "RENTED_TO");
    }

    #[test]
    fn test_edges() {
        let d = ErdDiagram {
            entities: vec![simple_entity("CUSTOMER")],
            relationships: vec![simple_relationship("RENTED_TO")],
            links: vec![simple_link("RENTED_TO", "CUSTOMER", "1")],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.edges.len(), 1);
        assert_eq!(layout.edges[0].from_id, "RENTED_TO");
        assert_eq!(layout.edges[0].to_id, "CUSTOMER");
        assert_eq!(layout.edges[0].label, "1");
    }

    #[test]
    fn test_multiple_entities_same_rank() {
        let d = ErdDiagram {
            entities: vec![simple_entity("A"), simple_entity("B"), simple_entity("C")],
            direction: ErdDirection::TopToBottom,
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.entity_nodes.len(), 3);
        // Unlinked → same rank → same y, increasing x
        let x0 = layout.entity_nodes[0].x;
        let x1 = layout.entity_nodes[1].x;
        let x2 = layout.entity_nodes[2].x;
        assert!(x0 < x1, "A.x < B.x: {} < {}", x0, x1);
        assert!(x1 < x2, "B.x < C.x: {} < {}", x1, x2);
    }

    #[test]
    fn test_left_to_right_direction() {
        let d = ErdDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            direction: ErdDirection::LeftToRight,
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        // In LR mode, two unlinked nodes should have the same x (same column)
        // but different y positions. The exact ordering depends on graphviz
        // internal layout decisions.
        let y0 = layout.entity_nodes[0].y;
        let y1 = layout.entity_nodes[1].y;
        assert!(
            (y0 - y1).abs() > 1.0,
            "A and B should be at different y in LR: A.y={}, B.y={}",
            y0,
            y1
        );
    }

    #[test]
    fn test_weak_entity() {
        let d = ErdDiagram {
            entities: vec![ErdEntity {
                is_weak: true,
                ..simple_entity("CHILD")
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert!(layout.entity_nodes[0].is_weak);
    }

    #[test]
    fn test_identifying_relationship() {
        let d = ErdDiagram {
            relationships: vec![ErdRelationship {
                is_identifying: true,
                ..simple_relationship("PARENT_OF")
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert!(layout.relationship_nodes[0].is_identifying);
    }

    #[test]
    fn test_bounding_box() {
        let d = ErdDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            relationships: vec![simple_relationship("R")],
            links: vec![simple_link("R", "A", "1"), simple_link("R", "B", "N")],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        for node in layout
            .entity_nodes
            .iter()
            .chain(layout.relationship_nodes.iter())
        {
            assert!(node.x + node.width <= layout.width);
            assert!(node.y + node.height <= layout.height);
        }
    }

    #[test]
    fn test_nested_attributes() {
        let d = ErdDiagram {
            entities: vec![ErdEntity {
                id: "DIR".to_string(),
                name: "DIRECTOR".to_string(),
                attributes: vec![ErdAttribute {
                    name: "Name".to_string(),
                    display_name: None,
                    is_key: false,
                    is_derived: false,
                    is_multi: false,
                    attr_type: None,
                    children: vec![simple_attr("Fname"), simple_attr("Lname")],
                    color: None,
                    source_line: 0,
                }],
                is_weak: false,
                color: None,
                source_order: 0,
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.attribute_nodes.len(), 1);
        assert_eq!(layout.attribute_nodes[0].children.len(), 2);
    }

    #[test]
    fn test_double_edge() {
        let d = ErdDiagram {
            entities: vec![simple_entity("A")],
            relationships: vec![simple_relationship("R")],
            links: vec![ErdLink {
                from: "R".to_string(),
                to: "A".to_string(),
                cardinality: "N".to_string(),
                is_double: true,
                color: None,
                isa_arrow: None,
                source_order: 2,
                source_line: 0,
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert!(layout.edges[0].is_double);
    }

    #[test]
    fn test_clip_to_rect_below() {
        let (x, y) = clip_to_rect(100.0, 100.0, 80.0, 40.0, 100.0, 200.0);
        assert!((x - 100.0).abs() < 1.0);
        assert!((y - 120.0).abs() < 1.0);
    }

    #[test]
    fn test_clip_to_rect_right() {
        let (x, y) = clip_to_rect(100.0, 100.0, 80.0, 40.0, 300.0, 100.0);
        assert!((x - 140.0).abs() < 1.0);
        assert!((y - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_isa_layout() {
        let d = ErdDiagram {
            entities: vec![
                simple_entity("PARENT"),
                simple_entity("CHILD1"),
                simple_entity("CHILD2"),
            ],
            isas: vec![ErdIsa {
                parent: "PARENT".to_string(),
                kind: IsaKind::Disjoint,
                children: vec!["CHILD1".to_string(), "CHILD2".to_string()],
                is_double: true,
                color: None,
                source_order: 3,
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert_eq!(layout.isa_layouts.len(), 1);
        assert_eq!(layout.isa_layouts[0].kind_label, "d");
        assert_eq!(layout.isa_layouts[0].child_edges.len(), 2);
        assert!(layout.isa_layouts[0].is_double);
        // ISA center should be positioned by graphviz
        assert!(layout.isa_layouts[0].radius > 0.0);
    }

    #[test]
    fn test_derived_attribute() {
        let d = ErdDiagram {
            entities: vec![ErdEntity {
                id: "E".to_string(),
                name: "E".to_string(),
                attributes: vec![ErdAttribute {
                    is_derived: true,
                    ..simple_attr("Bonus")
                }],
                is_weak: false,
                color: None,
                source_order: 0,
            }],
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        assert!(layout.attribute_nodes[0].is_derived);
    }

    #[test]
    fn test_topology_ranks() {
        let d = ErdDiagram {
            entities: vec![simple_entity("A"), simple_entity("B")],
            relationships: vec![simple_relationship("R")],
            links: vec![simple_link("A", "R", "1"), simple_link("R", "B", "N")],
            direction: ErdDirection::TopToBottom,
            ..empty_diagram()
        };
        let layout = layout_erd(&d).unwrap();
        let ay = layout.entity_nodes.iter().find(|n| n.id == "A").unwrap().y;
        let ry = layout
            .relationship_nodes
            .iter()
            .find(|n| n.id == "R")
            .unwrap()
            .y;
        let by = layout.entity_nodes.iter().find(|n| n.id == "B").unwrap().y;
        assert!(ay < ry, "A.y < R.y: {} < {}", ay, ry);
        assert!(ry < by, "R.y < B.y: {} < {}", ry, by);
    }
}
