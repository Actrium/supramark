//! d2-exporter: convert a d2-graph `Graph` into a d2-target `Diagram` for rendering.
//!
//! Ported from Go `d2exporter/export.go`.

// ---------------------------------------------------------------------------
// Export entry point
// ---------------------------------------------------------------------------

/// Export a compiled and laid-out d2 graph to a renderable diagram.
pub fn export(
    g: &crate::graph::Graph,
    font_family: Option<crate::fonts::FontFamily>,
    mono_font_family: Option<crate::fonts::FontFamily>,
) -> Result<crate::target::Diagram, String> {
    let mut diagram = crate::target::Diagram::new();

    // Apply root styles
    apply_styles(&mut diagram.root, &g.objects[g.root]);
    let root_obj = &g.objects[g.root];
    if root_obj.label.map_key.is_none() {
        diagram.root.text.label = g.name.clone();
    } else {
        diagram.root.text.label = root_obj.label.value.clone();
    }
    diagram.name = g.name.clone();
    diagram.is_folder_only = g.is_folder_only;

    // Font family selection
    let mut effective_font = font_family.unwrap_or(crate::fonts::FontFamily::SourceSansPro);
    if let Some(ref theme) = g.theme
        && theme.special_rules.mono
    {
        effective_font = crate::fonts::FontFamily::SourceCodePro;
    }
    diagram.font_family = Some(effective_font.to_string());
    diagram.mono_font_family = Some(
        mono_font_family
            .unwrap_or(crate::fonts::FontFamily::SourceCodePro)
            .to_string(),
    );

    // Convert objects to shapes (skip root and class field/method
    // placeholders that have been absorbed into their parent class).
    let exportable_objects: Vec<&crate::graph::Object> = g
        .objects
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != g.root)
        .map(|(_, obj)| obj)
        .filter(|obj| obj.shape.value != "__d2_class_field_removed__")
        .collect();

    diagram.shapes = exportable_objects
        .iter()
        .map(|obj| to_shape(obj, g))
        .collect();

    // Convert edges to connections (skip edges with no route, e.g. unresolved group edges)
    diagram.connections = g
        .edges
        .iter()
        .filter(|edge| edge.route.len() >= 2)
        .map(|edge| to_connection(edge, g))
        .collect();

    // Handle legend. Legend objects/edges were compiled in a scratch graph,
    // so `edge.src`/`edge.dst` are indices into that scratch graph's object
    // array — a direct `g.objects[edge.src]` lookup in `to_connection` would
    // panic when the scratch graph has more objects than the main one. We
    // redirect src/dst to main root during conversion, then overwrite the
    // resulting connection's `src`/`dst` strings using `legend.object_abs_ids`
    // (indexed by the scratch ObjId).
    if let Some(ref legend) = g.legend {
        let mut target_legend = crate::target::Legend {
            label: legend.label.clone(),
            ..Default::default()
        };

        if !legend.objects.is_empty() {
            target_legend.shapes = legend.objects.iter().map(|obj| to_shape(obj, g)).collect();
        }

        if !legend.edges.is_empty() {
            target_legend.connections = legend
                .edges
                .iter()
                .map(|edge| {
                    let mut patched = edge.clone();
                    patched.src = g.root;
                    patched.dst = g.root;
                    let mut conn = to_connection(&patched, g);
                    conn.src = legend
                        .object_abs_ids
                        .get(edge.src)
                        .cloned()
                        .unwrap_or_default();
                    conn.dst = legend
                        .object_abs_ids
                        .get(edge.dst)
                        .cloned()
                        .unwrap_or_default();
                    conn
                })
                .collect();
        }

        diagram.legend = Some(target_legend);
    }

    Ok(diagram)
}

// ---------------------------------------------------------------------------
// Style application
// ---------------------------------------------------------------------------

/// Apply DSL style properties from an object to a shape.
fn apply_styles(shape: &mut crate::target::Shape, obj: &crate::graph::Object) {
    if let Some(ref v) = obj.style.opacity {
        shape.opacity = v.value.parse().unwrap_or(1.0);
    }
    if let Some(ref v) = obj.style.stroke_dash {
        shape.stroke_dash = v.value.parse().unwrap_or(0.0);
    }
    if let Some(ref v) = obj.style.fill {
        shape.fill = v.value.clone();
    } else if obj.shape.value == crate::target::SHAPE_TEXT {
        shape.fill = "transparent".to_owned();
    }
    if let Some(ref v) = obj.style.fill_pattern {
        shape.fill_pattern = v.value.clone();
    }
    if let Some(ref v) = obj.style.stroke {
        shape.stroke = v.value.clone();
    }
    if let Some(ref v) = obj.style.stroke_width {
        shape.stroke_width = v.value.parse().unwrap_or(2);
    }
    if let Some(ref v) = obj.style.shadow {
        shape.shadow = v.value == "true";
    }
    if let Some(ref v) = obj.style.three_dee {
        shape.three_dee = v.value == "true";
    }
    if let Some(ref v) = obj.style.multiple {
        shape.multiple = v.value == "true";
    }
    if let Some(ref v) = obj.style.border_radius {
        shape.border_radius = v.value.parse().unwrap_or(0);
    }
    if let Some(ref v) = obj.style.font_color {
        shape.text.color = v.value.clone();
    }
    if let Some(ref v) = obj.style.italic {
        shape.text.italic = v.value == "true";
    }
    if let Some(ref v) = obj.style.bold {
        shape.text.bold = v.value == "true";
    }
    if let Some(ref v) = obj.style.underline {
        shape.text.underline = v.value == "true";
    }
    if let Some(ref v) = obj.style.font {
        shape.text.font_family = v.value.clone();
    }
    if let Some(ref v) = obj.style.double_border {
        shape.double_border = v.value == "true";
    }
    if let Some(ref v) = obj.icon_style.border_radius {
        shape.icon_border_radius = v.value.parse().unwrap_or(0);
    }
}

/// Apply theme colors and special rules to a shape.
fn apply_theme(
    shape: &mut crate::target::Shape,
    obj: &crate::graph::Object,
    theme: Option<&crate::themes::Theme>,
    g: &crate::graph::Graph,
) {
    shape.stroke = obj.get_stroke(shape.stroke_dash).to_owned();
    shape.fill = obj.get_fill(g).to_owned();

    if obj.shape.value == crate::target::SHAPE_TEXT {
        shape.text.color = crate::color::N1.to_owned();
    }
    if obj.shape.value == crate::target::SHAPE_SQL_TABLE
        || obj.shape.value == crate::target::SHAPE_CLASS
    {
        shape.primary_accent_color = crate::color::B2.to_owned();
        shape.secondary_accent_color = crate::color::AA2.to_owned();
        shape.neutral_accent_color = crate::color::N2.to_owned();
    }

    if let Some(theme) = theme {
        let level = obj.level(g);
        let is_container = obj.is_container();

        if theme.special_rules.outer_container_double_border && level == 1 && is_container {
            shape.double_border = true;
        }
        if theme.special_rules.container_dots && is_container {
            shape.fill_pattern = "dots".to_owned();
        } else if theme.special_rules.all_paper {
            shape.fill_pattern = "paper".to_owned();
        }
        if theme.special_rules.mono {
            shape.text.font_family = "mono".to_owned();
        }
        if theme.special_rules.c4 {
            if is_container {
                if obj.style.fill.is_none() {
                    shape.fill = "transparent".to_owned();
                }
                if obj.style.stroke.is_none() {
                    shape.stroke = crate::color::AA2.to_owned();
                }
                if obj.style.stroke_dash.is_none() {
                    shape.stroke_dash = 5.0;
                }
                if obj.style.font_color.is_none() {
                    shape.text.color = crate::color::N1.to_owned();
                }
            }
            if level == 1
                && !is_container
                && obj.shape.value != crate::target::SHAPE_PERSON
                && obj.shape.value != crate::target::SHAPE_C4_PERSON
            {
                if obj.style.fill.is_none() {
                    shape.fill = crate::color::B6.to_owned();
                }
                if obj.style.stroke.is_none() {
                    shape.stroke = crate::color::B5.to_owned();
                }
            }
            if obj.shape.value == crate::target::SHAPE_PERSON
                || obj.shape.value == crate::target::SHAPE_C4_PERSON
            {
                if obj.style.fill.is_none() {
                    shape.fill = crate::color::B2.to_owned();
                }
                if obj.style.stroke.is_none() {
                    shape.stroke = crate::color::B1.to_owned();
                }
            }
            if level > 1
                && !is_container
                && obj.shape.value != crate::target::SHAPE_PERSON
                && obj.shape.value != crate::target::SHAPE_C4_PERSON
            {
                if obj.style.fill.is_none() {
                    shape.fill = crate::color::B4.to_owned();
                }
                if obj.style.stroke.is_none() {
                    shape.stroke = crate::color::B3.to_owned();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Object -> Shape
// ---------------------------------------------------------------------------

fn to_shape(obj: &crate::graph::Object, g: &crate::graph::Graph) -> crate::target::Shape {
    let mut shape = crate::target::base_shape();
    shape.set_type(&obj.shape.value);
    shape.id = obj.abs_id().to_owned();
    shape.classes = obj.classes.clone();
    shape.z_index = obj.z_index;
    shape.level = obj.level(g) as i32;
    shape.pos = crate::target::Point::new(obj.top_left.x as i32, obj.top_left.y as i32);
    shape.width = obj.width as i32;
    shape.height = obj.height as i32;
    shape.text.language = obj.language.clone();

    let text = obj.text(g);
    shape.text.bold = text.is_bold;
    shape.text.italic = text.is_italic;
    shape.text.font_size = text.font_size;

    if obj.is_sequence_diagram() {
        shape.stroke_width = 0;
    }
    if obj.is_sequence_diagram_group() {
        shape.stroke_width = 0;
        shape.blend = true;
    }

    // Apply styles twice (matching Go behavior: first pass, then theme, then again)
    apply_styles(&mut shape, obj);
    apply_theme(&mut shape, obj, g.theme.as_ref(), g);

    // Text color from measured text
    let is_italic = shape.text.italic;
    shape.text.color = text.get_color(is_italic).to_owned();

    if let Some(ref theme) = g.theme
        && theme.special_rules.c4
        && obj.style.font_color.is_none()
    {
        if obj.is_container() {
            shape.text.color = crate::color::N1.to_owned();
        } else {
            shape.text.color = crate::color::N7.to_owned();
        }
    }

    // Second apply_styles (overrides theme defaults with explicit user styles)
    apply_styles(&mut shape, obj);

    // Shape-specific handling
    match obj.shape.value.to_ascii_lowercase().as_str() {
        crate::target::SHAPE_CLASS => {
            if let Some(ref class) = obj.class {
                shape.class = class.clone();
            }
            shape.text.font_size -= crate::target::HEADER_FONT_ADD;
        }
        crate::target::SHAPE_SQL_TABLE => {
            if let Some(ref table) = obj.sql_table {
                shape.sql_table = table.clone();
            }
            shape.text.font_size -= crate::target::HEADER_FONT_ADD;
        }
        crate::target::SHAPE_CLOUD => {
            if let Some(ratio) = obj.content_aspect_ratio {
                shape.content_aspect_ratio = Some(ratio);
            }
        }
        _ => {}
    }

    shape.text.label = text.text;
    shape.text.label_width = text.dimensions.width;
    shape.text.label_height = text.dimensions.height;

    if let Some(ref pos) = obj.label_position {
        shape.label_position = pos.clone();
        if obj.is_sequence_diagram_group() {
            shape.text.label_fill = shape.fill.clone();
        }
    }

    if let Some(ref tooltip) = obj.tooltip {
        shape.tooltip = tooltip.value.clone();
    }
    if let Some(ref pos) = obj.tooltip_position {
        shape.tooltip_position = pos.clone();
    }
    if let Some(ref v) = obj.style.animated {
        shape.animated = v.value == "true";
    }
    if let Some(ref link) = obj.link {
        shape.link = link.value.clone();
        shape.pretty_link = link.value.clone(); // simplified: no link prettification
    }
    shape.icon = obj.icon.clone();
    if let Some(ref pos) = obj.icon_position {
        shape.icon_position = pos.clone();
    }

    shape
}

// ---------------------------------------------------------------------------
// Edge -> Connection
// ---------------------------------------------------------------------------

fn to_connection(edge: &crate::graph::Edge, g: &crate::graph::Graph) -> crate::target::Connection {
    let mut conn = crate::target::base_connection();
    conn.id = edge.abs_id().to_owned();
    conn.classes = edge.classes.clone();
    conn.z_index = edge.z_index;

    let text = edge.text();

    // Source arrowhead
    if edge.src_arrow {
        conn.src_arrow = crate::target::Arrowhead::DEFAULT;
        if let Some(ref ah) = edge.src_arrowhead {
            conn.src_arrow = ah.to_arrowhead();
        }
    }
    if let Some(ref ah) = edge.src_arrowhead
        && !ah.label.value.is_empty()
    {
        conn.src_label = Some(crate::target::Text {
            label: ah.label.value.clone(),
            label_width: ah.label_dimensions.width,
            label_height: ah.label_dimensions.height,
            color: ah
                .style
                .font_color
                .as_ref()
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ..Default::default()
        });
    }

    // Destination arrowhead
    if edge.dst_arrow {
        conn.dst_arrow = crate::target::Arrowhead::DEFAULT;
        if let Some(ref ah) = edge.dst_arrowhead {
            conn.dst_arrow = ah.to_arrowhead();
        }
    }
    if let Some(ref ah) = edge.dst_arrowhead
        && !ah.label.value.is_empty()
    {
        conn.dst_label = Some(crate::target::Text {
            label: ah.label.value.clone(),
            label_width: ah.label_dimensions.width,
            label_height: ah.label_dimensions.height,
            color: ah
                .style
                .font_color
                .as_ref()
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ..Default::default()
        });
    }

    // Theme corner radius override
    if let Some(ref theme) = g.theme
        && theme.special_rules.no_corner_radius
    {
        conn.border_radius = 0.0;
    }

    // Edge style overrides
    if let Some(ref v) = edge.style.border_radius {
        conn.border_radius = v.value.parse().unwrap_or(10.0);
    }
    if let Some(ref v) = edge.style.opacity {
        conn.opacity = v.value.parse().unwrap_or(1.0);
    }
    if let Some(ref v) = edge.style.stroke_dash {
        conn.stroke_dash = v.value.parse().unwrap_or(0.0);
    }

    conn.stroke = edge.get_stroke(conn.stroke_dash).to_owned();
    if let Some(ref v) = edge.style.stroke {
        conn.stroke = v.value.clone();
    }
    if let Some(ref v) = edge.style.stroke_width {
        conn.stroke_width = v.value.parse().unwrap_or(2);
    }
    if let Some(ref v) = edge.style.fill {
        conn.fill = v.value.clone();
    }

    conn.text.font_size = text.font_size;
    if let Some(ref v) = edge.style.font_size {
        conn.text.font_size = v.value.parse().unwrap_or(16);
    }
    if let Some(ref v) = edge.style.animated {
        conn.animated = v.value == "true";
    }

    if let Some(ref tooltip) = edge.tooltip {
        conn.tooltip = tooltip.value.clone();
    }
    if let Some(ref icon) = edge.icon {
        conn.icon = Some(icon.clone());
        if let Some(ref pos) = edge.icon_position {
            conn.icon_position = pos.clone();
        } else {
            conn.icon_position = crate::label::Position::InsideMiddleCenter
                .as_str()
                .to_owned();
        }
    }
    if let Some(ref v) = edge.icon_style.border_radius {
        conn.icon_border_radius = v.value.parse().unwrap_or(0.0);
    }
    if let Some(ref v) = edge.style.italic {
        conn.text.italic = v.value == "true";
    }

    // Text color
    conn.text.color = text.get_color(conn.text.italic).to_owned();
    if let Some(ref v) = edge.style.font_color {
        conn.text.color = v.value.clone();
    }
    if let Some(ref v) = edge.style.bold {
        conn.text.bold = v.value == "true";
    }
    if let Some(ref v) = edge.style.underline {
        conn.text.underline = v.value == "true";
    }

    if let Some(ref theme) = g.theme
        && theme.special_rules.mono
    {
        conn.text.font_family = "mono".to_owned();
    }
    if let Some(ref v) = edge.style.font {
        conn.text.font_family = v.value.clone();
    }
    if let Some(ref link) = edge.link {
        conn.link = link.value.clone();
    }

    conn.text.label = text.text;
    conn.text.label_width = text.dimensions.width;
    conn.text.label_height = text.dimensions.height;
    conn.text.language = edge.language.clone();

    if let Some(ref pos) = edge.label_position {
        conn.label_position = pos.clone();
    }
    if let Some(pct) = edge.label_percentage {
        conn.label_percentage = pct as f64;
    }

    // Route: truncate decimals and float32 precision
    conn.route = edge
        .route
        .iter()
        .map(|p| {
            let mut pt = *p;
            pt.truncate_decimals();
            pt.truncate_float32();
            crate::geo::Point::new(pt.x, pt.y)
        })
        .collect();

    conn.is_curve = edge.is_curve;

    conn.src = g.objects[edge.src].abs_id().to_owned();
    conn.dst = if let Some(ref ovr) = edge.dst_id_override {
        ovr.clone()
    } else {
        g.objects[edge.dst].abs_id().to_owned()
    };

    // C4 theme overrides for connections
    if let Some(ref theme) = g.theme
        && theme.special_rules.c4
    {
        if edge.style.stroke_dash.is_none() {
            conn.stroke_dash = 5.0;
        }
        if edge.style.stroke.is_none() {
            conn.stroke = crate::color::AA4.to_owned();
        }
        if edge.style.font_color.is_none() {
            conn.text.color = crate::color::N2.to_owned();
        }
    }

    conn
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Edge, Graph, Label, Object};

    /// Basic export: 2 nodes, 1 edge.
    #[test]
    fn export_simple_graph() {
        let mut g = Graph::new();
        g.name = "test".to_owned();

        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            label: Label {
                value: "Node A".into(),
                ..Default::default()
            },
            width: 100.0,
            height: 50.0,
            top_left: crate::geo::Point::new(10.0, 20.0),
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            label: Label {
                value: "Node B".into(),
                ..Default::default()
            },
            width: 100.0,
            height: 50.0,
            top_left: crate::geo::Point::new(10.0, 120.0),
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            dst_arrow: true,
            label: Label {
                value: "connects".into(),
                ..Default::default()
            },
            route: vec![
                crate::geo::Point::new(60.0, 70.0),
                crate::geo::Point::new(60.0, 120.0),
            ],
            ..Default::default()
        });

        let diagram = export(&g, None, None).expect("export failed");

        assert_eq!(diagram.name, "test");
        assert_eq!(diagram.shapes.len(), 2);
        assert_eq!(diagram.connections.len(), 1);

        // Check shapes
        assert_eq!(diagram.shapes[0].id, "a");
        assert_eq!(diagram.shapes[0].text.label, "Node A");
        assert_eq!(diagram.shapes[0].pos.x, 10);
        assert_eq!(diagram.shapes[0].pos.y, 20);
        assert_eq!(diagram.shapes[0].width, 100);
        assert_eq!(diagram.shapes[0].height, 50);

        assert_eq!(diagram.shapes[1].id, "b");
        assert_eq!(diagram.shapes[1].text.label, "Node B");

        // Check connection
        let conn = &diagram.connections[0];
        assert_eq!(conn.id, "(a -> b)[0]");
        assert_eq!(conn.src, "a");
        assert_eq!(conn.dst, "b");
        assert_eq!(conn.text.label, "connects");
        assert!(!conn.route.is_empty());
    }

    /// Export with arrowheads.
    #[test]
    fn export_with_arrowheads() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "x".into(),
            abs_id: "x".into(),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "y".into(),
            abs_id: "y".into(),
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });
        g.add_edge(Edge {
            abs_id: "(x -> y)[0]".into(),
            src: a,
            dst: b,
            src_arrow: true,
            dst_arrow: true,
            route: vec![
                crate::geo::Point::new(80.0, 20.0),
                crate::geo::Point::new(160.0, 20.0),
            ],
            ..Default::default()
        });

        let diagram = export(&g, None, None).unwrap();
        let conn = &diagram.connections[0];
        assert_ne!(conn.src_arrow, crate::target::Arrowhead::None);
        assert_ne!(conn.dst_arrow, crate::target::Arrowhead::None);
    }

    /// Export with a theme applied.
    #[test]
    fn export_with_theme() {
        let mut g = Graph::new();
        g.theme = Some(crate::themes::Theme {
            id: 0,
            name: "test-theme",
            colors: crate::themes::ColorPalette {
                neutrals: crate::themes::COOL_NEUTRAL,
                b1: "#000",
                b2: "#111",
                b3: "#222",
                b4: "#333",
                b5: "#444",
                b6: "#555",
                aa2: "#666",
                aa4: "#777",
                aa5: "#888",
                ab4: "#999",
                ab5: "#aaa",
            },
            special_rules: crate::themes::SpecialRules {
                mono: true,
                ..Default::default()
            },
        });

        let _a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            label: Label {
                value: "Mono".into(),
                ..Default::default()
            },
            width: 80.0,
            height: 40.0,
            ..Default::default()
        });

        let diagram = export(&g, None, None).unwrap();
        // Font family should be set to mono because of theme
        assert_eq!(diagram.font_family.as_deref(), Some("SourceCodePro"));
        // Shape font should be mono
        assert_eq!(diagram.shapes[0].text.font_family, "mono");
    }

    /// Export empty graph.
    #[test]
    fn export_empty_graph() {
        let g = Graph::new();
        let diagram = export(&g, None, None).unwrap();
        assert!(diagram.shapes.is_empty());
        assert!(diagram.connections.is_empty());
    }

    /// Export with style overrides.
    #[test]
    fn export_with_style_overrides() {
        let mut g = Graph::new();
        let _a = g.add_object(Object {
            id: "styled".into(),
            abs_id: "styled".into(),
            width: 100.0,
            height: 50.0,
            style: crate::graph::Style {
                fill: Some(crate::graph::ScalarValue {
                    value: "#ff0000".into(),
                }),
                stroke: Some(crate::graph::ScalarValue {
                    value: "#00ff00".into(),
                }),
                opacity: Some(crate::graph::ScalarValue {
                    value: "0.5".into(),
                }),
                shadow: Some(crate::graph::ScalarValue {
                    value: "true".into(),
                }),
                ..Default::default()
            },
            ..Default::default()
        });

        let diagram = export(&g, None, None).unwrap();
        let shape = &diagram.shapes[0];
        assert_eq!(shape.fill, "#ff0000");
        assert_eq!(shape.stroke, "#00ff00");
        assert!((shape.opacity - 0.5).abs() < f64::EPSILON);
        assert!(shape.shadow);
    }
}
