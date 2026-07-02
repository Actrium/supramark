use std::fmt::Write;

use crate::font_metrics;
use crate::klimt::drawable::{
    DrawStyle, Drawable, EllipseShape, LineShape, PolygonShape, RectShape,
};
use crate::klimt::svg::{fmt_coord, xml_escape, LengthAdjust, SvgGraphic};
use crate::layout::activity::{
    classify_activity_table_lines, ActivityEdgeKindLayout, ActivityEdgeLayout, ActivityLayout,
    ActivityNodeKindLayout, ActivityNodeLayout, ActivityNoteModeLayout, ActivityTableKind,
    NotePositionLayout, SwimlaneLayout, TABLE_CELL_PADDING,
};
use crate::model::activity::ActivityDiagram;
use crate::render::svg::{
    compute_viewport, ensure_visible_int, write_bg_rect, write_svg_root_bg, ViewportConfig,
};
use crate::render::svg_richtext::{
    creole_line_height, creole_text_width, get_sprite_svg, render_creole_display_lines,
    render_creole_text, render_creole_text_opts, render_creole_text_word_by_word,
};
use crate::render::svg_sprite;
use crate::style::SkinParams;
use crate::Result;

// -- Style constants (PlantUML rose theme) ------------------------------------

/// Note font size (Java: 13px for notes)
const NOTE_FONT_SIZE: f64 = 13.0;
/// Action/diamond font size (Java: 12px, from activityDiagram.activity.FontSize)
const ACTION_FONT_SIZE: f64 = 12.0;
/// Java `Swimlanes.swimlanesSpecial()` appends an empty dummy swimlane. Its
/// title block still contributes a final divider `x2 = 7.86083984375`, so the
/// transparent title background extends `x2 - 6` past the visible right divider.
const SWIMLANE_TITLE_BG_RIGHT_EXTRA: f64 = 1.86083984375;
/// Action text line height – computed from font_metrics, matching
/// Java's AWT `font.getStringBounds().getHeight()` = ascent + descent.
fn action_line_height() -> f64 {
    font_metrics::line_height("SansSerif", ACTION_FONT_SIZE, false, false)
}

use crate::skin::rose::{
    BORDER_COLOR, ENTITY_BG, FORK_FILL, INITIAL_FILL, NOTE_BG, NOTE_BORDER, TEXT_COLOR,
};

// -- Public entry point -------------------------------------------------------

/// Render an activity diagram to SVG.
///
/// `body_offset` is an optional `(dx, dy)` offset applied to all body
/// coordinates.  When the body is wrapped by `wrap_with_meta`, this bakes
/// the meta-element offset directly into the coordinates, avoiding the
/// lossy string-level `offset_svg_coords` post-processing.
///
/// Returns `(svg_string, raw_body_dimensions)`.  The raw width is the
/// precise layout width (avoids integer-truncation centering drift); the
/// raw height is derived from the SVG viewport so that the existing
/// `wrap_with_meta` height formula stays balanced.
pub fn render_activity(
    diagram: &ActivityDiagram,
    layout: &ActivityLayout,
    skin: &SkinParams,
    body_offset: Option<(f64, f64)>,
) -> Result<(String, Option<(f64, f64)>)> {
    use crate::model::activity::ActivityEvent;

    // Skin color lookups
    let bg = skin.get_or("backgroundcolor", "#FFFFFF");
    let act_bg = skin.background_color("activity", ENTITY_BG);
    let act_border = skin.border_color("activity", BORDER_COLOR);
    let act_font = skin.font_color("activity", TEXT_COLOR);
    let diamond_bg = skin.background_color("activityDiamond", ENTITY_BG);
    let diamond_border = skin.border_color("activityDiamond", BORDER_COLOR);
    let swimlane_border = skin.border_color("swimlane", TEXT_COLOR);
    let swimlane_font = skin.font_color("swimlane", TEXT_COLOR);
    let arrow_color = skin.arrow_color(BORDER_COLOR);

    // Java: when MaximumWidth is set, notes use word-by-word rendering
    // (each word and space becomes a separate <text> SVG element).
    let word_by_word_notes = diagram.note_max_width.is_some();

    // Body offset: when wrapping with meta, coordinates are shifted so that
    // wrap_with_meta can include the body content directly without lossy
    // string-level coordinate shifting.
    let (bo_x, bo_y) = body_offset.unwrap_or((0.0, 0.0));
    log::debug!(
        "render_activity: layout {:.2}x{:.2}, old_style_graphviz={}",
        layout.width,
        layout.height,
        layout.old_style_graphviz
    );

    if layout.old_style_graphviz {
        return render_old_style_activity(layout, skin, body_offset);
    }

    // --- Build node→lane mapping (same logic as layout Pass 2c) -----------
    let mut node_lane: Vec<usize> = Vec::new();
    let mut cur_lane: usize = 0;
    for event in &diagram.events {
        match event {
            ActivityEvent::Swimlane { name } => {
                cur_lane = diagram
                    .swimlanes
                    .iter()
                    .position(|n| n == name)
                    .unwrap_or(0);
            }
            _ => {
                node_lane.push(cur_lane);
            }
        }
    }

    // --- Shift layout data by body offset (for meta wrapping) ---------------
    // When body_offset is set, all coordinates are pre-shifted so that
    // wrap_with_meta can include the body content at body_abs_y = 0.
    let shifted_nodes: Vec<ActivityNodeLayout>;
    let shifted_edges: Vec<ActivityEdgeLayout>;
    let nodes_ref: &[ActivityNodeLayout];
    let edges_ref: &[ActivityEdgeLayout];
    if bo_x.abs() > 0.001 || bo_y.abs() > 0.001 {
        shifted_nodes = layout
            .nodes
            .iter()
            .map(|n| {
                let mut sn = n.clone();
                sn.x += bo_x;
                sn.y += bo_y;
                sn
            })
            .collect();
        shifted_edges = layout
            .edges
            .iter()
            .map(|e| {
                let mut se = e.clone();
                se.points = se
                    .points
                    .iter()
                    .map(|&(px, py)| (px + bo_x, py + bo_y))
                    .collect();
                se
            })
            .collect();
        nodes_ref = &shifted_nodes;
        edges_ref = &shifted_edges;
    } else {
        nodes_ref = &layout.nodes;
        edges_ref = &layout.edges;
    }

    // --- Render into SvgGraphic (Java drawWhenSwimlanes order) ------------
    let mut sg = SvgGraphic::new(0, 1.0);
    sg.track_rect(0.0, 0.0, layout.width + bo_x, layout.height + bo_y);

    let has_swimlanes = !layout.swimlane_layouts.is_empty();

    if has_swimlanes {
        // Stable Java's swimlane title block sits 5px higher than our older
        // approximation, so the transparent header background and the first
        // divider line must start 5px earlier as well.
        let header_asc = font_metrics::ascent("SansSerif", 18.0, false, false);
        let header_desc = font_metrics::descent("SansSerif", 18.0, false, false);
        let titles_height = header_asc + header_desc;
        let header_top = 2019.0 / 2048.0 * 18.0 - 5.0;

        // Step 1: drawTitlesBackground — transparent header rect
        if let (Some(first), Some(last)) = (
            layout.swimlane_layouts.first(),
            layout.swimlane_layouts.last(),
        ) {
            sg.push_raw(&format!(
                r#"<rect fill="none" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fmt_coord(titles_height),
                fmt_coord(last.x + last.width - first.x + SWIMLANE_TITLE_BG_RIGHT_EXTRA),
                fmt_coord(first.x),
                fmt_coord(header_top),
            ));
        }

        // Compute content bottom (max node y+h) for divider line extent.
        // Java swimlane dividers stop at the content bottom, not the SVG
        // viewport bottom (which includes margins).
        let content_bottom = nodes_ref
            .iter()
            .map(|n| n.y + n.height)
            .fold(0.0_f64, f64::max);

        // Step 2: per-lane content + divider line
        for (lane_idx, sw) in layout.swimlane_layouts.iter().enumerate() {
            // 2a: Render nodes belonging to this lane
            let mut ni = 0usize;
            while ni < nodes_ref.len() {
                let nl = node_lane.get(ni).copied().unwrap_or(0);
                if nl != lane_idx {
                    ni += 1;
                    continue;
                }

                let node = &nodes_ref[ni];
                match &node.kind {
                    ActivityNodeKindLayout::Note { .. }
                    | ActivityNodeKindLayout::FloatingNote { .. } => {
                        render_node(
                            &mut sg,
                            node,
                            act_bg,
                            act_border,
                            act_font,
                            diamond_bg,
                            diamond_border,
                            arrow_color,
                            word_by_word_notes,
                        );
                        ni += 1;
                    }
                    _ => {
                        let mut left_notes = Vec::new();
                        let mut right_notes = Vec::new();
                        let mut j = ni + 1;
                        while j < nodes_ref.len() {
                            let lane_j = node_lane.get(j).copied().unwrap_or(0);
                            if lane_j != lane_idx {
                                break;
                            }
                            match &nodes_ref[j].kind {
                                ActivityNodeKindLayout::Note { position, .. }
                                | ActivityNodeKindLayout::FloatingNote { position, .. } => {
                                    match position {
                                        NotePositionLayout::Left => left_notes.push(j),
                                        NotePositionLayout::Right => right_notes.push(j),
                                    }
                                }
                                _ => break,
                            }
                            j += 1;
                        }

                        for idx in left_notes.into_iter().chain(right_notes.into_iter()) {
                            render_node(
                                &mut sg,
                                &nodes_ref[idx],
                                act_bg,
                                act_border,
                                act_font,
                                diamond_bg,
                                diamond_border,
                                arrow_color,
                                word_by_word_notes,
                            );
                        }
                        render_node(
                            &mut sg,
                            node,
                            act_bg,
                            act_border,
                            act_font,
                            diamond_bg,
                            diamond_border,
                            arrow_color,
                            word_by_word_notes,
                        );
                        ni = j;
                    }
                }
            }
            // 2b: Divider line (Java: y1=header_top, y2=content_bottom)
            let swim_border_style = DrawStyle::outline(swimlane_border, 1.5);
            LineShape {
                x1: sw.x,
                y1: header_top,
                x2: sw.x,
                y2: content_bottom,
            }
            .draw(&mut sg, &swim_border_style);
        }
        // Right border line
        if let Some(last) = layout.swimlane_layouts.last() {
            let right_x = last.x + last.width;
            LineShape {
                x1: right_x,
                y1: header_top,
                x2: right_x,
                y2: content_bottom,
            }
            .draw(&mut sg, &DrawStyle::outline(swimlane_border, 1.5));
        }

        // Step 3: same-lane edges are emitted during the per-lane draw pass;
        // cross-lane edges are emitted afterwards by Swimlanes.Cross.
        for lane_idx in 0..layout.swimlane_layouts.len() {
            for edge in edges_ref {
                let from_lane = node_lane.get(edge.from_index).copied().unwrap_or(0);
                let to_lane = node_lane.get(edge.to_index).copied().unwrap_or(0);
                if from_lane == lane_idx && to_lane == lane_idx {
                    render_edge(&mut sg, edge, arrow_color, act_font);
                }
            }
        }
        for edge in edges_ref {
            let from_lane = node_lane.get(edge.from_index).copied().unwrap_or(0);
            let to_lane = node_lane.get(edge.to_index).copied().unwrap_or(0);
            if from_lane != to_lane {
                render_edge(&mut sg, edge, arrow_color, act_font);
            }
        }

        // Step 4: header text (drawn last)
        // Java uses `CenteredText(swTitle, getWidthWithoutTitle(swimlane))`.
        // In Rust's lane model the visual lane width includes the 10px divider
        // band, so `getWidthWithoutTitle(swimlane)` maps to `sw.width - 10`.
        let header_text_y = header_top + header_asc;
        for sw in &layout.swimlane_layouts {
            let tl = font_metrics::text_width(&sw.name, "SansSerif", 18.0, false, false);
            let label_x = sw.x + 5.0 + ((sw.width - 10.0) - tl) / 2.0;
            sg.set_fill_color(swimlane_font);
            sg.svg_text(
                &sw.name,
                label_x,
                header_text_y,
                Some("sans-serif"),
                18.0,
                None,
                None,
                None,
                tl,
                LengthAdjust::Spacing,
                None,
                0,
                None,
            );
        }
    } else {
        // No swimlanes: Java draws nodes first, then edges (connections).
        // When a `render_order` permutation is present (e.g. for `repeat`
        // blocks), walk nodes in that order so the repeat body is drawn
        // before diamond1/hex — matching Java `FtileRepeat.drawU`.
        if let Some(order) = layout.render_order.as_ref() {
            for &pos in order {
                if let Some(node) = nodes_ref.get(pos) {
                    render_node(
                        &mut sg,
                        node,
                        act_bg,
                        act_border,
                        act_font,
                        diamond_bg,
                        diamond_border,
                        arrow_color,
                        word_by_word_notes,
                    );
                }
            }
        } else {
            for node in nodes_ref {
                render_node(
                    &mut sg,
                    node,
                    act_bg,
                    act_border,
                    act_font,
                    diamond_bg,
                    diamond_border,
                    arrow_color,
                    word_by_word_notes,
                );
            }
        }
        for edge in edges_ref {
            render_edge(&mut sg, edge, arrow_color, act_font);
        }
    }

    // --- SVG dimensions from ensureVisible tracking (Java compat) ----------
    let (max_x, max_y) = sg.max_dimensions();
    log::debug!("ensureVisible: maxX={max_x} maxY={max_y}");
    let svg_w = max_x as f64;
    let svg_h = max_y as f64;

    // --- Assemble final SVG: header + body --------------------------------
    let mut buf = String::with_capacity(4096);
    write_svg_root_bg(&mut buf, svg_w, svg_h, "ACTIVITY", bg);
    buf.push_str("<defs/><g>");
    write_bg_rect(&mut buf, svg_w, svg_h, bg);
    buf.push_str(sg.body());
    buf.push_str("</g></svg>");

    // Post-process: inject back-highlight filter definitions
    let filters = crate::render::svg_richtext::take_back_filters();
    if !filters.is_empty() {
        let mut defs_content = String::new();
        for (id, hex_color) in &filters {
            use std::fmt::Write;
            write!(
                defs_content,
                r#"<filter height="1" id="{}" width="1" x="0" y="0"><feFlood flood-color="{}" result="flood"/><feComposite in="SourceGraphic" in2="flood" operator="over"/></filter>"#,
                id, hex_color,
            ).unwrap();
        }
        buf = buf.replacen("<defs/>", &format!("<defs>{}</defs>", defs_content), 1);
    }

    // Raw body dimensions for wrap_with_meta:
    // - width: precise layout width (float) for exact title centering
    // - height: derived from the ORIGINAL layout SVG viewport (without body
    //   offset) minus doc margins, matching the extraction formula so
    //   canvas_h stays balanced.
    let doc_margin_top_activity = 10.0_f64;
    let doc_margin_bottom = 5.0_f64;
    // Use the original layout height (not shifted) for dimension recovery.
    let orig_svg_h = ensure_visible_int(layout.height) as f64;
    let raw_body_h = orig_svg_h - doc_margin_top_activity - doc_margin_bottom - 1.0;
    let raw_body_dim = Some((layout.width, raw_body_h));

    Ok((buf, raw_body_dim))
}

fn render_old_style_activity(
    layout: &ActivityLayout,
    skin: &SkinParams,
    body_offset: Option<(f64, f64)>,
) -> Result<(String, Option<(f64, f64)>)> {
    let bg = skin.get_or("backgroundcolor", "#FFFFFF");
    let act_bg = skin.background_color("activity", ENTITY_BG);
    let act_border = skin.border_color("activity", BORDER_COLOR);
    let act_font = skin.font_color("activity", TEXT_COLOR);
    let diamond_bg = skin.background_color("activityDiamond", ENTITY_BG);
    let diamond_border = skin.border_color("activityDiamond", BORDER_COLOR);
    let arrow_color = skin.arrow_color(BORDER_COLOR);
    let (bo_x, bo_y) = body_offset.unwrap_or((0.0, 0.0));
    let old_bo_y = bo_y - 1.0;

    let mut body = String::with_capacity(8192);
    let mut max_x = 0.0_f64;
    let mut max_y = 0.0_f64;

    for (idx, node) in layout.nodes.iter().enumerate() {
        let meta = layout.old_node_meta.get(idx).and_then(|meta| meta.as_ref());
        render_old_style_node(
            &mut body,
            node,
            meta,
            bo_x,
            old_bo_y,
            act_bg,
            act_border,
            act_font,
            diamond_bg,
            diamond_border,
            &mut max_x,
            &mut max_y,
        );
    }

    for (idx, edge) in layout.edges.iter().enumerate() {
        let meta = layout.old_edge_meta.get(idx).and_then(|meta| meta.as_ref());
        render_old_style_edge(
            &mut body,
            edge,
            meta,
            bo_x,
            old_bo_y,
            arrow_color,
            &mut max_x,
            &mut max_y,
        );
    }

    // Old Graphviz-backed activities: Java adds doc margins (right=5, bottom=5)
    // plus LimitFinder's +1 compensation. Combined margin = 5 on each side.
    let (svg_w, svg_h) = compute_viewport(max_x, max_y, &ViewportConfig::ACTIVITY_OLD);

    let mut buf = String::with_capacity(body.len() + 256);
    write_svg_root_bg(&mut buf, svg_w, svg_h, "ACTIVITY", bg);
    buf.push_str("<defs/><g>");
    write_bg_rect(&mut buf, svg_w, svg_h, bg);
    buf.push_str(&body);
    buf.push_str("</g></svg>");

    Ok((buf, Some((svg_w, svg_h))))
}

#[allow(clippy::too_many_arguments)]
fn render_old_style_node(
    buf: &mut String,
    node: &ActivityNodeLayout,
    meta: Option<&crate::layout::activity::ActivityGraphvizNodeMeta>,
    bo_x: f64,
    bo_y: f64,
    act_bg: &str,
    act_border: &str,
    act_font: &str,
    diamond_bg: &str,
    diamond_border: &str,
    max_x: &mut f64,
    max_y: &mut f64,
) {
    let x = node.x + bo_x;
    let y = node.y + bo_y;
    let right = x + node.width;
    let bottom = y + node.height;
    *max_x = max_x.max(right);
    *max_y = max_y.max(bottom);

    let wrapper = match node.kind {
        ActivityNodeKindLayout::Start => Some("start_entity"),
        ActivityNodeKindLayout::Stop | ActivityNodeKindLayout::End => Some("end_entity"),
        ActivityNodeKindLayout::Diamond => Some("entity"),
        _ => None,
    };
    if let (Some(class_name), Some(meta)) = (wrapper, meta) {
        // Java emits `data-source-line` on the wrapper tag of `start_entity`
        // and `end_entity`, but NOT on the neutral `entity` (diamond)
        // wrapper. Match that behavior exactly.
        let emit_source_line =
            matches!(class_name, "start_entity" | "end_entity") && meta.source_line.is_some();
        if emit_source_line {
            write!(
                buf,
                r#"<g class="{}" data-qualified-name="{}" data-source-line="{}" id="{}">"#,
                class_name,
                xml_escape(&meta.qualified_name),
                meta.source_line.unwrap(),
                meta.uid,
            )
            .unwrap();
        } else {
            write!(
                buf,
                r#"<g class="{}" data-qualified-name="{}" id="{}">"#,
                class_name,
                xml_escape(&meta.qualified_name),
                meta.uid,
            )
            .unwrap();
        }
    }

    match node.kind {
        ActivityNodeKindLayout::Start => {
            write!(
                buf,
                r##"<ellipse cx="{}" cy="{}" fill="#222222" rx="10" ry="10" style="stroke:#222222;stroke-width:1;"/>"##,
                fmt_coord(x + node.width / 2.0),
                fmt_coord(y + node.height / 2.0),
            )
            .unwrap();
        }
        ActivityNodeKindLayout::Stop | ActivityNodeKindLayout::End => {
            let cx = x + node.width / 2.0;
            let cy = y + node.height / 2.0;
            write!(
                buf,
                r#"<ellipse cx="{}" cy="{}" fill="none" rx="11" ry="11" style="stroke:#222222;stroke-width:1.5;"/>"#,
                fmt_coord(cx),
                fmt_coord(cy),
            )
            .unwrap();
            write!(
                buf,
                r##"<ellipse cx="{}" cy="{}" fill="#222222" rx="6" ry="6" style="stroke:#222222;stroke-width:1;"/>"##,
                fmt_coord(cx),
                fmt_coord(cy),
            )
            .unwrap();
        }
        ActivityNodeKindLayout::Action | ActivityNodeKindLayout::BackwardAction => {
            write!(
                buf,
                r#"<rect fill="{}" height="{}" rx="12.5" ry="12.5" style="stroke:{};stroke-width:0.5;" width="{}" x="{}" y="{}"/>"#,
                act_bg,
                fmt_coord(node.height),
                act_border,
                fmt_coord(node.width),
                fmt_coord(x),
                fmt_coord(y),
            )
            .unwrap();
            let text_y =
                y + 10.0 + font_metrics::ascent("SansSerif", ACTION_FONT_SIZE, false, false);
            let text_w =
                font_metrics::text_width(&node.text, "SansSerif", ACTION_FONT_SIZE, false, false);
            write!(
                buf,
                r#"<text fill="{}" font-family="sans-serif" font-size="12" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"#,
                act_font,
                fmt_coord(text_w),
                fmt_coord(x + 10.0),
                fmt_coord(text_y),
                xml_escape(&node.text),
            )
            .unwrap();
            *max_x = max_x.max(x + 10.0 + text_w);
            *max_y = max_y.max(text_y);
        }
        ActivityNodeKindLayout::Diamond => {
            let cx = x + node.width / 2.0;
            let cy = y + node.height / 2.0;
            write!(
                buf,
                r#"<polygon fill="{}" points="{},{},{},{},{},{},{},{},{},{}" style="stroke:{};stroke-width:0.5;"/>"#,
                diamond_bg,
                fmt_coord(cx),
                fmt_coord(y),
                fmt_coord(x + node.width),
                fmt_coord(cy),
                fmt_coord(cx),
                fmt_coord(y + node.height),
                fmt_coord(x),
                fmt_coord(cy),
                fmt_coord(cx),
                fmt_coord(y),
                diamond_border,
            )
            .unwrap();
        }
        ActivityNodeKindLayout::SyncBar | ActivityNodeKindLayout::ForkBar => {
            write!(
                buf,
                r#"<rect fill="{SYNC_BAR_FILL}" height="{}" style="stroke:none;stroke-width:1;" width="{}" x="{}" y="{}"/>"#,
                fmt_coord(node.height),
                fmt_coord(node.width),
                fmt_coord(x),
                fmt_coord(y),
            )
            .unwrap();
        }
        _ => {}
    }

    if wrapper.is_some() && meta.is_some() {
        buf.push_str("</g>");
    }
}

#[allow(clippy::too_many_arguments)]
fn render_old_style_edge(
    buf: &mut String,
    edge: &ActivityEdgeLayout,
    meta: Option<&crate::layout::activity::ActivityGraphvizEdgeMeta>,
    bo_x: f64,
    bo_y: f64,
    arrow_color: &str,
    max_x: &mut f64,
    max_y: &mut f64,
) {
    let Some(meta) = meta else {
        return;
    };
    let edge_ascent = font_metrics::ascent("SansSerif", 11.0, false, false);

    write!(buf, "<!--link {} to {}-->", meta.from_id, meta.to_id).unwrap();
    write!(
        buf,
        r#"<g class="link" data-entity-1="{}" data-entity-2="{}" data-link-type="dependency" data-source-line="{}" id="{}">"#,
        meta.from_uid, meta.to_uid, meta.source_line, meta.uid
    )
    .unwrap();

    if let Some(path_d) = old_activity_path_d(edge, meta.raw_path_d.as_deref(), bo_x, bo_y) {
        write!(
            buf,
            r#"<path d="{}" fill="none" id="{}-to-{}" style="stroke:{};stroke-width:1;"/>"#,
            path_d, meta.from_id, meta.to_id, arrow_color,
        )
        .unwrap();
    }

    if let Some(points) = old_activity_arrow_polygon(edge, bo_x, bo_y) {
        let mut points_attr = String::new();
        for (idx, (x, y)) in points.iter().enumerate() {
            if idx > 0 {
                points_attr.push(',');
            }
            points_attr.push_str(&fmt_coord(*x));
            points_attr.push(',');
            points_attr.push_str(&fmt_coord(*y));
            *max_x = max_x.max(*x);
            *max_y = max_y.max(*y);
        }
        write!(
            buf,
            r#"<polygon fill="{}" points="{}" style="stroke:{};stroke-width:1;"/>"#,
            arrow_color, points_attr, arrow_color,
        )
        .unwrap();
    }

    if !edge.label.is_empty() {
        if let Some((x, y)) = meta.label_xy {
            let x = x + bo_x + 1.0;
            let y = y + bo_y + edge_ascent + 1.0;
            let text_w = font_metrics::text_width(&edge.label, "SansSerif", 11.0, false, false);
            write!(
                buf,
                r##"<text fill="#000000" font-family="sans-serif" font-size="11" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"##,
                fmt_coord(text_w),
                fmt_coord(x),
                fmt_coord(y),
                xml_escape(&edge.label),
            )
            .unwrap();
            *max_x = max_x.max(x + text_w);
            *max_y = max_y.max(y);
        }
    }

    if let (Some(head_label), Some((x, y))) = (&meta.head_label, meta.head_label_xy) {
        let x = x + bo_x;
        let y = y + bo_y + edge_ascent;
        let display = if head_label.is_empty() {
            " "
        } else {
            head_label.as_str()
        };
        let text_w = font_metrics::text_width(display, "SansSerif", 11.0, false, false);
        let text = if head_label.is_empty() {
            "&#160;".to_string()
        } else {
            xml_escape(head_label)
        };
        write!(
            buf,
            r##"<text fill="#000000" font-family="sans-serif" font-size="11" lengthAdjust="spacing" textLength="{}" x="{}" y="{}">{}</text>"##,
            fmt_coord(text_w),
            fmt_coord(x),
            fmt_coord(y),
            text,
        )
        .unwrap();
        *max_x = max_x.max(x + text_w);
        *max_y = max_y.max(y);
    }

    buf.push_str("</g>");
}

fn old_activity_path_d(
    edge: &ActivityEdgeLayout,
    raw_path_d: Option<&str>,
    bo_x: f64,
    bo_y: f64,
) -> Option<String> {
    if edge.points.len() >= 4 && (edge.points.len() - 1) % 3 == 0 {
        let mut pts: Vec<(f64, f64)> = edge
            .points
            .iter()
            .map(|&(x, y)| (x + bo_x, y + bo_y))
            .collect();
        let n = pts.len();
        let (c2x, c2y) = pts[n - 2];
        let (end_x, end_y) = pts[n - 1];
        let dx = end_x - c2x;
        let dy = end_y - c2y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > f64::EPSILON {
            let trim = 6.0_f64.min(len);
            let ux = dx / len;
            let uy = dy / len;
            pts[n - 2] = (c2x - ux * trim, c2y - uy * trim);
            pts[n - 1] = (end_x - ux * trim, end_y - uy * trim);
        }

        let mut d = String::new();
        d.push('M');
        d.push_str(&fmt_coord(pts[0].0));
        d.push(',');
        d.push_str(&fmt_coord(pts[0].1));
        for chunk in pts[1..].chunks(3) {
            if chunk.len() != 3 {
                return raw_path_d.map(|raw| {
                    if bo_x.abs() > 0.001 || bo_y.abs() > 0.001 {
                        crate::layout::graphviz::transform_path_d(raw, bo_x, bo_y)
                    } else {
                        raw.to_string()
                    }
                });
            }
            d.push_str(" C");
            d.push_str(&fmt_coord(chunk[0].0));
            d.push(',');
            d.push_str(&fmt_coord(chunk[0].1));
            d.push(' ');
            d.push_str(&fmt_coord(chunk[1].0));
            d.push(',');
            d.push_str(&fmt_coord(chunk[1].1));
            d.push(' ');
            d.push_str(&fmt_coord(chunk[2].0));
            d.push(',');
            d.push_str(&fmt_coord(chunk[2].1));
        }
        return Some(d);
    }

    raw_path_d.map(|raw| {
        if bo_x.abs() > 0.001 || bo_y.abs() > 0.001 {
            crate::layout::graphviz::transform_path_d(raw, bo_x, bo_y)
        } else {
            raw.to_string()
        }
    })
}

fn old_activity_arrow_polygon(
    edge: &ActivityEdgeLayout,
    bo_x: f64,
    bo_y: f64,
) -> Option<[(f64, f64); 5]> {
    if edge.points.len() < 2 {
        return None;
    }
    let (fx, fy) = edge.points[edge.points.len() - 2];
    let (cx, cy) = edge.points[edge.points.len() - 1];
    let dx = cx - fx;
    let dy = cy - fy;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f64::EPSILON {
        return None;
    }

    let ux = dx / len;
    let uy = dy / len;
    let px = -uy;
    let py = ux;
    let tip_x = cx + bo_x;
    let tip_y = cy + bo_y;
    let left_x = cx - ux * 9.0 - px * 4.0 + bo_x;
    let left_y = cy - uy * 9.0 - py * 4.0 + bo_y;
    let notch_x = cx - ux * 5.0 + bo_x;
    let notch_y = cy - uy * 5.0 + bo_y;
    let right_x = cx - ux * 9.0 + px * 4.0 + bo_x;
    let right_y = cy - uy * 9.0 + py * 4.0 + bo_y;

    Some([
        (tip_x, tip_y),
        (left_x, left_y),
        (notch_x, notch_y),
        (right_x, right_y),
        (tip_x, tip_y),
    ])
}

// -- Node rendering -----------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn render_node(
    sg: &mut SvgGraphic,
    node: &ActivityNodeLayout,
    act_bg: &str,
    act_border: &str,
    act_font: &str,
    diamond_bg: &str,
    diamond_border: &str,
    arrow_color: &str,
    word_by_word_notes: bool,
) {
    match &node.kind {
        ActivityNodeKindLayout::Start => render_start(sg, node),
        ActivityNodeKindLayout::Stop => render_stop(sg, node),
        ActivityNodeKindLayout::End => render_stop(sg, node),
        ActivityNodeKindLayout::Action | ActivityNodeKindLayout::BackwardAction => {
            render_action(sg, node, act_bg, act_border, act_font)
        }
        ActivityNodeKindLayout::Diamond => render_diamond(sg, node, diamond_bg, diamond_border),
        ActivityNodeKindLayout::Hexagon {
            east_lines,
            south_lines,
        } => render_hexagon(
            sg,
            node,
            east_lines,
            south_lines,
            diamond_bg,
            diamond_border,
            act_font,
        ),
        ActivityNodeKindLayout::ForkBar => render_fork_bar(sg, node),
        ActivityNodeKindLayout::SyncBar => render_sync_bar(sg, node),
        ActivityNodeKindLayout::Note { position, mode } => {
            render_note(sg, node, position, mode, true, word_by_word_notes)
        }
        ActivityNodeKindLayout::FloatingNote { position, mode } => {
            render_note(sg, node, position, mode, false, word_by_word_notes)
        }
        ActivityNodeKindLayout::Detach => render_detach(sg, node, arrow_color),
        ActivityNodeKindLayout::IfDiamond {
            left_label,
            right_label,
            bottom_label,
        } => render_if_diamond(
            sg,
            node,
            left_label,
            right_label,
            bottom_label,
            diamond_bg,
            diamond_border,
            act_font,
        ),
        ActivityNodeKindLayout::GotoLines { segments } => {
            let line_style = DrawStyle::outline(arrow_color, 1.0);
            for &(x1, y1, x2, y2) in segments {
                LineShape { x1, y1, x2, y2 }.draw(sg, &line_style);
            }
        }
    }
}

/// Start node: filled ellipse
fn render_start(sg: &mut SvgGraphic, node: &ActivityNodeLayout) {
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    EllipseShape {
        cx,
        cy,
        rx: 10.0,
        ry: 10.0,
    }
    .draw(sg, &DrawStyle::filled(INITIAL_FILL, INITIAL_FILL, 1.0));
}

/// Stop / End node: double ellipse (outer ring + inner filled)
fn render_stop(sg: &mut SvgGraphic, node: &ActivityNodeLayout) {
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    EllipseShape {
        cx,
        cy,
        rx: 11.0,
        ry: 11.0,
    }
    .draw(sg, &DrawStyle::outline(INITIAL_FILL, 1.0));
    EllipseShape {
        cx,
        cy,
        rx: 6.0,
        ry: 6.0,
    }
    .draw(sg, &DrawStyle::filled(INITIAL_FILL, INITIAL_FILL, 1.0));
}

/// Action node: rounded rectangle with (possibly multi-line) text
fn render_action(
    sg: &mut SvgGraphic,
    node: &ActivityNodeLayout,
    bg: &str,
    border: &str,
    font_color: &str,
) {
    RectShape {
        x: node.x,
        y: node.y,
        w: node.width,
        h: node.height,
        rx: 12.5,
        ry: 12.5,
    }
    .draw(sg, &DrawStyle::filled(bg, border, 0.5));

    // Java: each line is a separate <text> element (not one <text> with <tspan>).
    // This matches Java's FtileBox rendering where SheetBlock1 draws each
    // Stripe/Atom as a separate UText draw call.
    let lines: Vec<&str> = node.text.split('\n').collect();
    // Java FtileBox: horizontalAlignment = LEFT, translate by (padding, padding).
    // base_x = rect_x + padding_left.
    let padding = 10.0; // Java: activityDiagram.activity.Padding = 10
    let base_x = node.x + padding;
    let top_y = node.y + padding;
    let baseline_offset = font_metrics::ascent("SansSerif", ACTION_FONT_SIZE, false, false);
    // Java DriverTextSvg: space width for leading-space offset
    let space_width = font_metrics::text_width(" ", "SansSerif", ACTION_FONT_SIZE, false, false);

    match classify_activity_table_lines(&lines) {
        Some(ActivityTableKind::MultiColumn) => {
            let display_lines: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
            let mut tmp = String::new();
            render_creole_display_lines(
                &mut tmp,
                &display_lines,
                base_x,
                top_y,
                font_color,
                r#"font-size="12""#,
                false,
            );
            sg.push_raw(&tmp);
            return;
        }
        Some(ActivityTableKind::SingleColumn { rows }) => {
            render_single_column_table_action(sg, base_x, top_y, font_color, &rows);
            return;
        }
        None => {}
    }

    let first_baseline = top_y + baseline_offset;
    let lh = action_line_height();

    // Java AtomImgSvg: sprite display scale = fontSize / (fontSize + 1)
    let sprite_scale = ACTION_FONT_SIZE / (ACTION_FONT_SIZE + 1.0);

    let mut y_cursor = 0.0_f64; // content height accumulated (relative to first_baseline area)
    for line in lines.iter() {
        let display_text = line.trim();
        if display_text.is_empty() {
            y_cursor += lh;
            continue;
        }

        // Check for sprite-only line: `<$name>`
        if display_text.starts_with("<$") && display_text.ends_with('>') {
            let inner = &display_text[2..display_text.len() - 1];
            let name = inner.split(',').next().unwrap_or(inner).trim();
            if let Some(svg_content) = get_sprite_svg(name) {
                let info = svg_sprite::sprite_info(&svg_content);
                let _sprite_w = info.vb_width * sprite_scale;
                let sprite_h = info.vb_height * sprite_scale;
                // Position sprite at base_x, current y
                let sprite_x = node.x + padding;
                let sprite_y = node.y + padding + y_cursor;
                // Activity diagrams use stroke-width:0.5 as the default.
                svg_sprite::set_default_stroke_width(0.5);
                // Java pre-computes absolute coordinates with scale applied,
                // instead of using <g transform> wrappers.
                let converted = svg_sprite::convert_svg_elements_scaled(
                    &svg_content,
                    sprite_x,
                    sprite_y,
                    sprite_scale,
                );
                sg.push_raw(&converted);
                svg_sprite::set_default_stroke_width(1.0); // restore default
                y_cursor += sprite_h;
                continue;
            }
        }

        let y = first_baseline + y_cursor;
        // Java DriverTextSvg algorithm:
        // 1. Count leading spaces → add space_width per space to x
        // 2. StringUtils.trin() the remaining text (strips trailing whitespace)
        // 3. Render trimmed text at adjusted x
        let leading_spaces = line.len() - line.trim_start_matches(' ').len();
        let text_x = base_x + leading_spaces as f64 * space_width;
        let mut tmp = String::new();
        // Java: activity action text preserves literal \n as displayable text
        render_creole_text_opts(
            &mut tmp,
            display_text,
            text_x,
            y,
            lh,
            font_color,
            None,
            r#"font-size="12""#,
            true, // preserve_backslash_n
        );
        sg.push_raw(&tmp);
        y_cursor += lh;
    }
}

fn render_single_column_table_action(
    sg: &mut SvgGraphic,
    x: f64,
    top_y: f64,
    font_color: &str,
    rows: &[String],
) {
    let row_heights: Vec<f64> = rows
        .iter()
        .map(|row| creole_line_height(row, "SansSerif", ACTION_FONT_SIZE))
        .collect();
    let content_width = rows.iter().fold(0.0_f64, |acc, row| {
        acc.max(creole_text_width(
            row,
            "SansSerif",
            ACTION_FONT_SIZE,
            false,
            false,
        ))
    });
    let ascent = font_metrics::ascent("SansSerif", ACTION_FONT_SIZE, false, false);
    let grid_top = top_y + TABLE_CELL_PADDING;
    let grid_bottom = grid_top + row_heights.iter().sum::<f64>();

    let mut row_top = grid_top;
    let mut tmp = String::new();
    for (row, row_height) in rows.iter().zip(&row_heights) {
        render_creole_text_opts(
            &mut tmp,
            row,
            x,
            row_top + ascent,
            *row_height,
            font_color,
            None,
            r#"font-size="12""#,
            false,
        );
        row_top += row_height;
    }

    let mut y = grid_top;
    write!(
        tmp,
        r#"<line style="stroke:#000000;stroke-width:0.5;" x1="{}" x2="{}" y1="{}" y2="{}"/>"#,
        fmt_coord(x),
        fmt_coord(x + content_width),
        fmt_coord(y),
        fmt_coord(y)
    )
    .unwrap();
    for row_height in &row_heights {
        y += row_height;
        write!(
            tmp,
            r#"<line style="stroke:#000000;stroke-width:0.5;" x1="{}" x2="{}" y1="{}" y2="{}"/>"#,
            fmt_coord(x),
            fmt_coord(x + content_width),
            fmt_coord(y),
            fmt_coord(y)
        )
        .unwrap();
    }
    write!(
        tmp,
        r#"<line style="stroke:#000000;stroke-width:0.5;" x1="{}" x2="{}" y1="{}" y2="{}"/>"#,
        fmt_coord(x),
        fmt_coord(x),
        fmt_coord(grid_top),
        fmt_coord(grid_bottom)
    )
    .unwrap();
    write!(
        tmp,
        r#"<line style="stroke:#000000;stroke-width:0.5;" x1="{}" x2="{}" y1="{}" y2="{}"/>"#,
        fmt_coord(x + content_width),
        fmt_coord(x + content_width),
        fmt_coord(grid_top),
        fmt_coord(grid_bottom)
    )
    .unwrap();

    sg.push_raw(&tmp);
}

/// Diamond node: rotated square for if/while conditions and `repeat` start.
///
/// Java `FtileDiamond` emits a `UPolygon` with the first vertex repeated to
/// close the shape, drawn through `borderColor.apply(stroke).bg(backColor)`.
/// stroke-width comes from the activity diamond style (0.5px in stable
/// PlantUML 1.2026.x).
///
/// When `node.text` is non-empty (e.g. a `while (condition)` diamond), the
/// condition text is centred inside the diamond — matching Java's
/// `FtileDiamondInside` behaviour for `while` loop conditions.
fn render_diamond(sg: &mut SvgGraphic, node: &ActivityNodeLayout, bg: &str, border: &str) {
    let x = node.x;
    let y = node.y;
    let w = node.width;
    let h = node.height;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    PolygonShape {
        points: vec![cx, y, x + w, cy, cx, y + h, x, cy, cx, y],
    }
    .draw(sg, &DrawStyle::filled(bg, border, 0.5));

    // Condition text centred inside the diamond (11pt sans-serif, matching the
    // hexagon/if-diamond condition font size).  Only drawn when present, so
    // the empty `repeat` start diamond is unaffected.
    //
    // `node.text` may carry multiple lines: `while (cond)` is single-line, but
    // an `elseif` diamond carries `"{condition}\n[{label}]"` (see
    // layout/activity.rs).  `svg_text` does not line-break, so we split on
    // `\n` and centre the whole block, rendering each line on its own baseline
    // — mirroring the hexagon south-label loop.  Without this the `\n`
    // collapses into one stretched line (review #44).
    if !node.text.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let lines: Vec<&str> = node.text.split('\n').collect();
        let block_h = line_h * lines.len() as f64;
        // Top of the centred text block.
        let block_top = y + (h - block_h) / 2.0;
        sg.set_fill_color(TEXT_COLOR);
        for (i, line) in lines.iter().enumerate() {
            let text_w = font_metrics::text_width(line, "SansSerif", font_size, false, false);
            let text_x = x + (w - text_w) / 2.0;
            let baseline_y = block_top + i as f64 * line_h + ascent;
            sg.svg_text(
                line,
                text_x,
                baseline_y,
                Some("sans-serif"),
                font_size,
                None,
                None,
                None,
                text_w,
                crate::klimt::svg::LengthAdjust::Spacing,
                None,
                0,
                None,
            );
        }
    }
}

/// Hexagonal diamond used by `repeat while (cond) is (label)`.
///
/// Java's `FtileDiamondInside` draws:
///   1. A six-vertex hexagon (`Hexagon.asPolygon(0, w, h)`).
///   2. The test condition (label) centred inside the hexagon — font 11pt.
///   3. The optional `is (...)` text as the East label, drawn line by line to
///      the right of the hexagon at `y = -dimEast.h + dimTotal.h / 2`.
fn render_hexagon(
    sg: &mut SvgGraphic,
    node: &ActivityNodeLayout,
    east_lines: &[String],
    south_lines: &[String],
    bg: &str,
    border: &str,
    font_color: &str,
) {
    let x = node.x;
    let y = node.y;
    let w = node.width;
    let h = node.height;
    let half = HEXAGON_RENDER_HALF_SIZE;

    // 1. Hexagon polygon (closed: first vertex repeated at the end).
    PolygonShape {
        points: vec![
            x + half,
            y,
            x + w - half,
            y,
            x + w,
            y + h / 2.0,
            x + w - half,
            y + h,
            x + half,
            y + h,
            x,
            y + h / 2.0,
            x + half,
            y,
        ],
    }
    .draw(sg, &DrawStyle::filled(bg, border, 0.5));

    // 2. South label: rendered before condition text (matches Java draw order).
    //    Java: `south.drawU(ug.apply(new UTranslate(4 + dimTotal.w/2, dimTotal.h)))`
    //    The text is left-aligned at x = hex_x + hex_w/2 + 4.
    if !south_lines.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let south_top_y = y + h;
        for (i, line) in south_lines.iter().enumerate() {
            let baseline_y = south_top_y + ascent + i as f64 * line_h;
            let text_w = font_metrics::text_width(line, "SansSerif", font_size, false, false);
            // Java: text left-aligned at hex_x + hex_w/2 + 4
            let text_x = x + w / 2.0 + 4.0;
            sg.set_fill_color(font_color);
            sg.svg_text(
                line,
                text_x,
                baseline_y,
                Some("sans-serif"),
                font_size,
                None,
                None,
                None,
                text_w,
                crate::klimt::svg::LengthAdjust::Spacing,
                None,
                0,
                None,
            );
        }
    }

    // 3. Inside test condition, centred inside the hexagon.  Font size matches
    //    Java's `FtileDiamondInside.label` (11pt sans-serif).
    if !node.text.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let text_w = font_metrics::text_width(&node.text, "SansSerif", font_size, false, false);
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let text_x = x + (w - text_w) / 2.0;
        let text_y = y + (h - line_h) / 2.0 + ascent;
        sg.set_fill_color(font_color);
        sg.svg_text(
            &node.text,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            text_w,
            crate::klimt::svg::LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }

    // 4. East label: each entry from `east_lines` is rendered as its own
    //    `<text>` to the right of the hexagon.  Java's
    //    `east.drawU(translate(dimTotal.w, -dimEast.h + dimTotal.h / 2))`
    //    places the text block above the hexagon vertical centre.
    if !east_lines.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let east_h = line_h * east_lines.len() as f64;
        let east_top_y = y + h / 2.0 - east_h;
        let east_x = x + w;
        for (i, line) in east_lines.iter().enumerate() {
            let baseline_y = east_top_y + ascent + i as f64 * line_h;
            // Empty trailing lines are emitted as a non-breaking space so the
            // viewport calculation matches Java's `LimitFinder` (which
            // measures the glyph box of the rendered space).
            let display_text = if line.is_empty() { " " } else { line.as_str() };
            let text_w =
                font_metrics::text_width(display_text, "SansSerif", font_size, false, false);
            sg.set_fill_color(font_color);
            sg.svg_text(
                if line.is_empty() { "\u{00A0}" } else { line },
                east_x,
                baseline_y,
                Some("sans-serif"),
                font_size,
                None,
                None,
                None,
                text_w,
                crate::klimt::svg::LengthAdjust::Spacing,
                None,
                0,
                None,
            );
        }
    }
}

/// Java `Hexagon.hexagonHalfSize` — duplicated here so the renderer is
/// independent of the layout module.
const HEXAGON_RENDER_HALF_SIZE: f64 = 12.0;
const HEXAGON_LABEL_FONT_SIZE_RENDER: f64 = 11.0;

/// If-diamond: hexagonal shape with condition text inside and branch labels.
///
/// Java's `FtileDiamondInside` draws the same hexagon as `Hexagon` but with
/// additional branch labels (left/right/bottom) for if/else conditions.
#[allow(clippy::too_many_arguments)]
fn render_if_diamond(
    sg: &mut SvgGraphic,
    node: &ActivityNodeLayout,
    left_label: &str,
    right_label: &str,
    bottom_label: &str,
    bg: &str,
    border: &str,
    font_color: &str,
) {
    let x = node.x;
    let y = node.y;
    let w = node.width;
    let h = node.height;
    let half = HEXAGON_RENDER_HALF_SIZE;

    // 1. Hexagon polygon (closed: first vertex repeated at the end).
    PolygonShape {
        points: vec![
            x + half,
            y,
            x + w - half,
            y,
            x + w,
            y + h / 2.0,
            x + w - half,
            y + h,
            x + half,
            y + h,
            x,
            y + h / 2.0,
            x + half,
            y,
        ],
    }
    .draw(sg, &DrawStyle::filled(bg, border, 0.5));

    // 2. Bottom label (south): rendered below the diamond center.
    //    Java's FtileDiamondInside draws south labels before the inner text.
    //    Java: x = cx + 4, y = diamond_bottom + ascent
    if !bottom_label.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let text_w = font_metrics::text_width(bottom_label, "SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let text_x = x + w / 2.0 + 4.0;
        let text_y = y + h + ascent;
        sg.set_fill_color(font_color);
        sg.svg_text(
            bottom_label,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            text_w,
            crate::klimt::svg::LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }

    // 3. Condition text, centred inside the hexagon (11pt sans-serif).
    if !node.text.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let text_w = font_metrics::text_width(&node.text, "SansSerif", font_size, false, false);
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let text_x = x + (w - text_w) / 2.0;
        let text_y = y + (h - line_h) / 2.0 + ascent;
        sg.set_fill_color(font_color);
        sg.svg_text(
            &node.text,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            text_w,
            crate::klimt::svg::LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }

    // 4. Left label (west): rendered to the left of the diamond.
    //    Java: west label at translate(-dimWest.w, -dimWest.h) from diamond center.
    //    baseline = cy - line_height + ascent
    if !left_label.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let text_w = font_metrics::text_width(left_label, "SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let cy = y + h / 2.0;
        let text_x = x - text_w;
        let text_y = cy - line_h + ascent;
        sg.set_fill_color(font_color);
        sg.svg_text(
            left_label,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            text_w,
            crate::klimt::svg::LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }

    // 5. Right label (east): rendered to the right of the diamond.
    //    Java: east label at translate(dimTotal.w, -dimEast.h) from diamond center.
    //    baseline = cy - line_height + ascent
    if !right_label.is_empty() {
        let font_size = HEXAGON_LABEL_FONT_SIZE_RENDER;
        let text_w = font_metrics::text_width(right_label, "SansSerif", font_size, false, false);
        let ascent = font_metrics::ascent("SansSerif", font_size, false, false);
        let line_h = font_metrics::line_height("SansSerif", font_size, false, false);
        let cy = y + h / 2.0;
        let text_x = x + w;
        let text_y = cy - line_h + ascent;
        sg.set_fill_color(font_color);
        sg.svg_text(
            right_label,
            text_x,
            text_y,
            Some("sans-serif"),
            font_size,
            None,
            None,
            None,
            text_w,
            crate::klimt::svg::LengthAdjust::Spacing,
            None,
            0,
            None,
        );
    }
}

/// Fork bar: thin black horizontal rectangle
fn render_fork_bar(sg: &mut SvgGraphic, node: &ActivityNodeLayout) {
    sg.push_raw(&format!(
        r#"<rect fill="{FORK_FILL}" height="{}" stroke="none" width="{}" x="{}" y="{}"/>"#,
        fmt_coord(node.height),
        fmt_coord(node.width),
        fmt_coord(node.x),
        fmt_coord(node.y),
    ));
}

/// Sync bar (old-style activity `===NAME===`): dark gray horizontal bar
const SYNC_BAR_FILL: &str = "#555555";

fn render_sync_bar(sg: &mut SvgGraphic, node: &ActivityNodeLayout) {
    sg.push_raw(&format!(
        "<rect fill=\"{SYNC_BAR_FILL}\" height=\"{}\" style=\"stroke:none;stroke-width:1;\" width=\"{}\" x=\"{}\" y=\"{}\"/>",
        fmt_coord(node.height),
        fmt_coord(node.width),
        fmt_coord(node.x),
        fmt_coord(node.y),
    ));
}

/// Detach node: an X marker
fn render_detach(sg: &mut SvgGraphic, node: &ActivityNodeLayout, arrow_color: &str) {
    let cx = node.x + node.width / 2.0;
    let cy = node.y + node.height / 2.0;
    let r = node.width / 2.0;
    let detach_style = DrawStyle::outline(arrow_color, 2.0);
    LineShape {
        x1: cx - r,
        y1: cy - r,
        x2: cx + r,
        y2: cy + r,
    }
    .draw(sg, &detach_style);
    LineShape {
        x1: cx + r,
        y1: cy - r,
        x2: cx - r,
        y2: cy + r,
    }
    .draw(sg, &detach_style);
}

/// Note (or floating note): path-based note shape with folded corner + text.
/// Single attached notes use Java's linked Opale polygon; grouped/floating
/// notes keep the normal folded-corner shape.
fn render_note(
    sg: &mut SvgGraphic,
    node: &ActivityNodeLayout,
    position: &NotePositionLayout,
    mode: &ActivityNoteModeLayout,
    linked: bool,
    word_by_word: bool,
) {
    let x = node.x;
    let y = node.y;
    let w = node.width;
    let h = node.height;
    let fold = 10.0;
    let margin_y = 5.0;
    let bullet_text_dx = 12.0;
    let bullet_radius = 2.5;

    let render_linked_opale = linked && *mode == ActivityNoteModeLayout::Single;
    if render_linked_opale {
        let cy = y + h / 2.0;
        let y1 = match position {
            NotePositionLayout::Right => (cy - 4.0).clamp(y, y + h - 8.0),
            NotePositionLayout::Left => (cy - 4.0).clamp(y + fold, y + h - 8.0),
        };
        let notch_left = x - 20.0;
        let notch_right = x + w + 20.0;
        let path_d = match position {
            NotePositionLayout::Right => format!(
                "M{},{} L{},{} L{},{} L{},{} L{},{} A0,0 0 0 0 {},{} L{},{} A0,0 0 0 0 {},{} L{},{} L{},{} L{},{} A0,0 0 0 0 {},{}",
                fmt_coord(x),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y1),
                fmt_coord(notch_left),
                fmt_coord(cy),
                fmt_coord(x),
                fmt_coord(y1 + 8.0),
                fmt_coord(x),
                fmt_coord(y + h),
                fmt_coord(x),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y + fold),
                fmt_coord(x + w - fold),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y)
            ),
            NotePositionLayout::Left => format!(
                "M{},{} L{},{} A0,0 0 0 0 {},{} L{},{} A0,0 0 0 0 {},{} L{},{} L{},{} L{},{} L{},{} L{},{} L{},{} A0,0 0 0 0 {},{}",
                fmt_coord(x),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y + h),
                fmt_coord(x),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y + h),
                fmt_coord(x + w),
                fmt_coord(y1 + 8.0),
                fmt_coord(notch_right),
                fmt_coord(cy),
                fmt_coord(x + w),
                fmt_coord(y1),
                fmt_coord(x + w),
                fmt_coord(y + fold),
                fmt_coord(x + w - fold),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y),
                fmt_coord(x),
                fmt_coord(y)
            ),
        };
        sg.push_raw(&format!(
            r#"<path d="{}" fill="{NOTE_BG}" style="stroke:{NOTE_BORDER};stroke-width:0.5;"/>"#,
            path_d
        ));
        // Track the callout protrusion so ensureVisible sees the linked shape.
        match position {
            NotePositionLayout::Right => sg.track_rect(x - 20.0, y, w + 20.0, h),
            NotePositionLayout::Left => sg.track_rect(x, y, w + 20.0, h),
        }
    } else {
        sg.push_raw(&format!(
            r#"<path d="M{},{} L{},{} L{},{} L{},{} L{},{} L{},{}" fill="{NOTE_BG}" style="stroke:{NOTE_BORDER};stroke-width:0.5;"/>"#,
            fmt_coord(x), fmt_coord(y),
            fmt_coord(x), fmt_coord(y + h),
            fmt_coord(x + w), fmt_coord(y + h),
            fmt_coord(x + w), fmt_coord(y + fold),
            fmt_coord(x + w - fold), fmt_coord(y),
            fmt_coord(x), fmt_coord(y),
        ));
        sg.track_rect(x, y, w, h);
    }

    // Fold triangle as <path>
    sg.push_raw(&format!(
        r#"<path d="M{},{} L{},{} L{},{} L{},{}" fill="{NOTE_BG}" style="stroke:{NOTE_BORDER};stroke-width:0.5;"/>"#,
        fmt_coord(x + w - fold), fmt_coord(y),
        fmt_coord(x + w - fold), fmt_coord(y + fold),
        fmt_coord(x + w), fmt_coord(y + fold),
        fmt_coord(x + w - fold), fmt_coord(y),
    ));

    // Render each line as a separate <text> element (matches Java's per-line rendering).
    // This avoids the multi-line textLength issue where a single <text> with tspans
    // gets an incorrect total textLength.
    let note_lh = crate::font_metrics::line_height("SansSerif", NOTE_FONT_SIZE, false, false);
    let note_ascent = crate::font_metrics::ascent("SansSerif", NOTE_FONT_SIZE, false, false);
    let note_descent = crate::font_metrics::descent("SansSerif", NOTE_FONT_SIZE, false, false);
    let text_x = x + 6.0;
    // Java Opale draws the SheetBlock translated by marginY, so the first
    // text baseline sits at marginY + ascent from the note top.
    let mut text_y = y + margin_y + note_ascent;
    let mut in_bullet_item = false;
    for line in node.text.split('\n') {
        // Horizontal separator gets less vertical space (Java: 10px)
        let trimmed = line.trim();
        let is_sep = trimmed.len() >= 4
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'));
        if is_sep {
            let sep_top = text_y - note_lh + note_descent;
            let sep_y1 = sep_top + 5.0;
            let sep_y2 = sep_y1 + 2.0;
            let sep_style = DrawStyle::outline(NOTE_BORDER, 1.0);
            LineShape {
                x1: x,
                y1: sep_y1,
                x2: x + w,
                y2: sep_y1,
            }
            .draw(sg, &sep_style);
            LineShape {
                x1: x,
                y1: sep_y2,
                x2: x + w,
                y2: sep_y2,
            }
            .draw(sg, &sep_style);
            in_bullet_item = false;
            text_y += crate::layout::activity::NOTE_SEPARATOR_HEIGHT;
            continue;
        }

        let (line_text, line_x) = if let Some(rest) = line.strip_prefix("* ") {
            let bullet_cx = text_x + 5.5;
            let bullet_cy = text_y - 4.4341;
            sg.push_raw(&format!(
                r#"<ellipse cx="{}" cy="{}" fill="{TEXT_COLOR}" rx="{}" ry="{}"/>"#,
                fmt_coord(bullet_cx),
                fmt_coord(bullet_cy),
                fmt_coord(bullet_radius),
                fmt_coord(bullet_radius),
            ));
            in_bullet_item = true;
            (rest, text_x + bullet_text_dx)
        } else if in_bullet_item && !trimmed.is_empty() {
            (line, text_x + bullet_text_dx)
        } else {
            in_bullet_item = false;
            (line, text_x)
        };

        let mut tmp = String::new();
        if word_by_word {
            render_creole_text_word_by_word(
                &mut tmp,
                line_text,
                line_x,
                text_y,
                note_lh,
                TEXT_COLOR,
                r#"font-size="13""#,
            );
        } else {
            render_creole_text(
                &mut tmp,
                line_text,
                line_x,
                text_y,
                note_lh,
                TEXT_COLOR,
                None,
                r#"font-size="13""#,
            );
        }
        sg.push_raw(&tmp);
        text_y += note_lh;
    }
}

// -- Edge rendering -----------------------------------------------------------

fn render_edge(
    sg: &mut SvgGraphic,
    edge: &ActivityEdgeLayout,
    arrow_color: &str,
    text_color: &str,
) {
    if edge.points.is_empty() {
        return;
    }

    // Special handling for `FtileRepeat.ConnectionBackSimple2` loop-back edges.
    if let ActivityEdgeKindLayout::LoopBackSimple2 { up_arrow_y } = edge.kind {
        render_loopback_simple2(sg, &edge.points, up_arrow_y, arrow_color);
        return;
    }

    // Backward loop-back edges: hex→backward (up arrow) and backward→diamond1 (left arrow)
    if matches!(
        edge.kind,
        ActivityEdgeKindLayout::LoopBackBackward1
            | ActivityEdgeKindLayout::LoopBackBackward2
            | ActivityEdgeKindLayout::GotoLoopBack
            | ActivityEdgeKindLayout::BreakEdge
    ) {
        render_polyline_with_arrow(sg, &edge.points, arrow_color);
        return;
    }

    // If-branch and if-merge edges: polyline with arrow at end
    if matches!(
        edge.kind,
        ActivityEdgeKindLayout::IfBranch | ActivityEdgeKindLayout::IfMerge
    ) {
        render_polyline_with_arrow(sg, &edge.points, arrow_color);
        return;
    }

    // If-merge with emphasize-direction DOWN arrow on the first long vertical
    // segment (implicit else when then-branch has break/goto).
    if matches!(edge.kind, ActivityEdgeKindLayout::IfMergeEmphasize) {
        render_if_merge_edge(sg, &edge.points, arrow_color);
        return;
    }

    // Goto no-arrow: polyline with no arrowhead
    if matches!(edge.kind, ActivityEdgeKindLayout::GotoNoArrow) {
        let line_style = DrawStyle::outline(arrow_color, 1.0);
        for pair in edge.points.windows(2) {
            let (x1, y1) = pair[0];
            let (x2, y2) = pair[1];
            LineShape { x1, y1, x2, y2 }.draw(sg, &line_style);
        }
        return;
    }

    // Render line segments
    let edge_line_style = DrawStyle::outline(arrow_color, 1.0);
    if edge.points.len() == 2 {
        let (x1, y1) = edge.points[0];
        let (x2, y2) = edge.points[1];
        LineShape { x1, y1, x2, y2 }.draw(sg, &edge_line_style);
    } else {
        // Multi-segment: render each segment as a separate <line>
        for pair in edge.points.windows(2) {
            let (x1, y1) = pair[0];
            let (x2, y2) = pair[1];
            LineShape { x1, y1, x2, y2 }.draw(sg, &edge_line_style);
        }
    }

    // Inline arrowhead polygon at the end of the edge
    if edge.points.len() >= 2 {
        let (tx, ty) = *edge.points.last().unwrap();
        let (fx, fy) = edge.points[edge.points.len() - 2];
        render_arrowhead(sg, fx, fy, tx, ty, arrow_color);
    }

    // Edge label (centered on midpoint)
    if !edge.label.is_empty() {
        let mid = edge.points.len() / 2;
        let (mx, my) = edge.points[mid];
        let tl = font_metrics::text_width(&edge.label, "SansSerif", ACTION_FONT_SIZE, false, false);
        sg.set_fill_color(text_color);
        sg.svg_text(
            &edge.label,
            mx,
            my,
            Some("sans-serif"),
            ACTION_FONT_SIZE,
            None,
            None,
            None,
            tl,
            LengthAdjust::Spacing,
            None,
            0,
            Some("middle"),
        );
    }
}

/// Render a polyline with an arrow at the end (last segment direction).
/// Used for backward, goto, and break edges.
fn render_polyline_with_arrow(sg: &mut SvgGraphic, points: &[(f64, f64)], arrow_color: &str) {
    if points.len() < 2 {
        return;
    }
    let edge_line_style = DrawStyle::outline(arrow_color, 1.0);

    // Draw line segments with Java's vertical normalization (y1 <= y2).
    for pair in points.windows(2) {
        let (x1, mut y1) = pair[0];
        let (x2, mut y2) = pair[1];
        if x1 == x2 && y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        LineShape { x1, y1, x2, y2 }.draw(sg, &edge_line_style);
    }

    // Arrow at the last point, direction from second-to-last to last.
    let (tx, ty) = *points.last().unwrap();
    let (fx, fy) = points[points.len() - 2];
    render_arrowhead(sg, fx, fy, tx, ty, arrow_color);
}

/// Render an IfMerge edge: polyline segments with a DOWN emphasize-direction
/// arrow on the first long vertical descending segment, plus a final arrow
/// at the end.  Matches Java's `Snake.emphasizeDirection(Direction.DOWN)`.
fn render_if_merge_edge(sg: &mut SvgGraphic, points: &[(f64, f64)], arrow_color: &str) {
    if points.len() < 2 {
        return;
    }
    let edge_line_style = DrawStyle::outline(arrow_color, 1.0);
    let poly_style = DrawStyle::filled(arrow_color, arrow_color, 1.0);

    // Find the first long vertical descending segment for emphasize arrow
    let mut emphasize_seg: Option<usize> = None;
    for (i, pair) in points.windows(2).enumerate() {
        let (x1, y1) = pair[0];
        let (x2, y2) = pair[1];
        if x1 == x2 && y2 > y1 && (y2 - y1) > 20.0 {
            emphasize_seg = Some(i);
            break;
        }
    }

    // Draw the first line segment (horizontal from diamond right)
    let (x1, y1) = points[0];
    let (x2, y2) = points[1];
    LineShape { x1, y1, x2, y2 }.draw(sg, &edge_line_style);

    // If the emphasize segment is the second segment (index 1), draw the
    // DOWN arrow polygon before the vertical line (matching Java's draw order).
    if let Some(seg_idx) = emphasize_seg {
        let (sx, _sy1) = points[seg_idx];
        let (_sx2, sy2) = points[seg_idx + 1];
        // Place the arrow tip near the bottom of the segment, 26px from bottom
        let arrow_tip_y = sy2 - 26.0;
        // DOWN arrow: tip at bottom, base 10px above tip
        PolygonShape {
            points: vec![
                sx - 4.0,
                arrow_tip_y - 10.0,
                sx,
                arrow_tip_y,
                sx + 4.0,
                arrow_tip_y - 10.0,
                sx,
                arrow_tip_y - 4.0,
            ],
        }
        .draw(sg, &poly_style);
    }

    // Draw remaining line segments with Java's vertical normalization (y1 <= y2)
    for pair in points.windows(2).skip(1) {
        let (x1, mut y1) = pair[0];
        let (x2, mut y2) = pair[1];
        if x1 == x2 && y1 > y2 {
            std::mem::swap(&mut y1, &mut y2);
        }
        LineShape { x1, y1, x2, y2 }.draw(sg, &edge_line_style);
    }

    // Arrow at the last point, direction from second-to-last to last.
    let (tx, ty) = *points.last().unwrap();
    let (fx, fy) = points[points.len() - 2];
    render_arrowhead(sg, fx, fy, tx, ty, arrow_color);
}

/// Render the 4-point `FtileRepeat.ConnectionBackSimple2` loop-back path.
///
/// Matches Java's `Snake(..., asToLeft()).emphasizeDirection(Direction.UP)`
/// output byte-for-byte:
///   1. horizontal line from the hex east side to `xmax`;
///   2. a mid-segment UP arrow polygon at `(xmax, up_arrow_y)` (drawn *before*
///      the vertical line in the XML output, matching Java's draw order);
///   3. the vertical line, with Y endpoints normalised so `y1 ≤ y2` (Java's
///      `UGraphicCompressOnXorY.drawLine` flips any UP segment);
///   4. horizontal line from `xmax` back into the diamond1 right edge;
///   5. the end LEFT arrow polygon at the final point.
fn render_loopback_simple2(
    sg: &mut SvgGraphic,
    points: &[(f64, f64)],
    up_arrow_y: f64,
    color: &str,
) {
    if points.len() != 4 {
        return;
    }
    let (x1, y1) = points[0];
    let (x2, y2) = points[1];
    let (x3, y3) = points[2];
    let (x4, y4) = points[3];

    let line_style = DrawStyle::outline(color, 1.0);
    let poly_style = DrawStyle::filled(color, color, 1.0);

    // Segment 1: horizontal from hex east.
    LineShape { x1, y1, x2, y2 }.draw(sg, &line_style);

    // Mid-segment UP arrow polygon (drawn BEFORE the vertical line, matching
    // Java's `Worm.drawLine` order: polygon, then line).  Uses Java
    // `ArrowsRegular.asToUp()` shape: tip at (0, 0), base at (±4, 10), notch
    // at (0, 6) — delta1=10, delta2=4.
    let ox = x2; // arrow x sits on the vertical segment
    let oy = up_arrow_y;
    PolygonShape {
        points: vec![
            ox - 4.0,
            oy + 10.0,
            ox,
            oy,
            ox + 4.0,
            oy + 10.0,
            ox,
            oy + 6.0,
        ],
    }
    .draw(sg, &poly_style);

    // Segment 2: vertical with Y normalisation (Java UGraphicCompressOnXorY).
    let (vy1, vy2) = if y2 > y3 { (y3, y2) } else { (y2, y3) };
    LineShape {
        x1: x2,
        y1: vy1,
        x2: x3,
        y2: vy2,
    }
    .draw(sg, &line_style);

    // Segment 3: horizontal back into diamond1 right.
    LineShape {
        x1: x3,
        y1: y3,
        x2: x4,
        y2: y4,
    }
    .draw(sg, &line_style);

    // End LEFT arrow polygon at the final point.  Java `asToLeft()` shape:
    // tip at (0, 0), points at (10, ±4), notch at (6, 0).
    let tx = x4;
    let ty = y4;
    PolygonShape {
        points: vec![
            tx + 10.0,
            ty - 4.0,
            tx,
            ty,
            tx + 10.0,
            ty + 4.0,
            tx + 6.0,
            ty,
        ],
    }
    .draw(sg, &poly_style);
}

/// Render an inline arrowhead polygon at the tip of an edge.
fn render_arrowhead(sg: &mut SvgGraphic, fx: f64, fy: f64, tx: f64, ty: f64, color: &str) {
    let dx = tx - fx;
    let dy = ty - fy;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let arrow_len = 10.0;
    let arrow_half = 4.0;

    // Java's UPolygon always puts the "negative cross-offset" point first.
    // For vertical arrows: first point is LEFT (x - 4).
    // For horizontal arrows: first point is TOP (y - 4).
    // We compute the perpendicular as (-|uy|, -|ux|) to ensure this.
    let px = if uy.abs() > 0.5 { -1.0 } else { 0.0 };
    let py = if ux.abs() > 0.5 { -1.0 } else { 0.0 };

    let lx = tx - ux * arrow_len + px * arrow_half;
    let ly = ty - uy * arrow_len + py * arrow_half;
    let rx = tx - ux * arrow_len - px * arrow_half;
    let ry = ty - uy * arrow_len - py * arrow_half;
    let mx = tx - ux * (arrow_len - 4.0);
    let my = ty - uy * (arrow_len - 4.0);
    PolygonShape {
        points: vec![lx, ly, tx, ty, rx, ry, mx, my],
    }
    .draw(sg, &DrawStyle::filled(color, color, 1.0));
}

// -- Swimlane rendering -------------------------------------------------------

#[allow(dead_code)] // reserved for swimlane rendering
fn render_swimlane(
    sg: &mut SvgGraphic,
    sw: &SwimlaneLayout,
    total_height: f64,
    border: &str,
    font_color: &str,
) {
    // Vertical divider line
    LineShape {
        x1: sw.x,
        y1: 0.0,
        x2: sw.x,
        y2: total_height,
    }
    .draw(sg, &DrawStyle::outline(border, 1.5));

    // Header label text (font-size 18 to match Java PlantUML)
    let label_x = sw.x + sw.width / 2.0;
    let tl = font_metrics::text_width(&sw.name, "SansSerif", 18.0, false, false);
    sg.set_fill_color(font_color);
    sg.svg_text(
        &sw.name,
        label_x,
        16.0,
        Some("sans-serif"),
        18.0,
        None,
        None,
        None,
        tl,
        LengthAdjust::Spacing,
        None,
        0,
        Some("middle"),
    );
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::activity::{
        ActivityEdgeKindLayout, ActivityEdgeLayout, ActivityLayout, ActivityNodeKindLayout,
        ActivityNodeLayout, NotePositionLayout, SwimlaneLayout,
    };
    use crate::model::activity::ActivityDiagram;
    use crate::style::SkinParams;

    fn empty_diagram() -> ActivityDiagram {
        ActivityDiagram {
            events: vec![],
            swimlanes: vec![],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        }
    }

    fn empty_layout() -> ActivityLayout {
        ActivityLayout {
            width: 200.0,
            height: 100.0,
            nodes: vec![],
            edges: vec![],
            swimlane_layouts: vec![],
            old_style_graphviz: false,
            old_node_meta: vec![],
            old_edge_meta: vec![],
            render_order: None,
        }
    }

    fn make_node(
        index: usize,
        kind: ActivityNodeKindLayout,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        text: &str,
    ) -> ActivityNodeLayout {
        ActivityNodeLayout {
            index,
            kind,
            x,
            y,
            width: w,
            height: h,
            text: text.to_string(),
            skip_in_flow: false,
        }
    }

    #[test]
    fn test_empty_diagram() {
        let diagram = empty_diagram();
        let layout = empty_layout();
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<defs/>"));
        assert!(!svg.contains("<ellipse"));
        assert!(!svg.contains("<rect"));
        assert!(!svg.contains("<line "));
    }

    #[test]
    fn test_start_ellipse() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Start,
            90.0,
            10.0,
            20.0,
            20.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(svg.contains(r#"rx="10""#), "start ellipse must have rx=10");
        assert!(svg.contains(r#"ry="10""#), "start ellipse must have ry=10");
        assert!(
            svg.contains(&format!(r#"fill="{INITIAL_FILL}""#)),
            "start ellipse must be filled"
        );
        assert_eq!(
            svg.matches("<ellipse").count(),
            1,
            "start node must produce exactly one ellipse"
        );
    }

    #[test]
    fn test_stop_ellipse() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Stop,
            90.0,
            80.0,
            22.0,
            22.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert_eq!(
            svg.matches("<ellipse").count(),
            2,
            "stop node must produce two ellipses"
        );
        assert!(
            svg.contains(r#"rx="11""#),
            "stop outer ring must have rx=11"
        );
        assert!(
            svg.contains(r#"rx="6""#),
            "stop inner ellipse must have rx=6"
        );
        assert!(
            svg.contains(r#"stroke-width:1;"#),
            "ellipses must have stroke-width=1"
        );
    }

    #[test]
    fn test_action_box() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Action,
            30.0,
            40.0,
            140.0,
            36.0,
            "Do something",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(r#"rx="12.5""#),
            "action must have rounded corners rx=12.5"
        );
        assert!(
            svg.contains(r#"ry="12.5""#),
            "action must have rounded corners ry=12.5"
        );
        assert!(
            svg.contains(r#"stroke-width:0.5;"#),
            "action border must be stroke-width 0.5"
        );
        assert!(
            svg.contains(r##"fill="#F1F1F1""##),
            "action must use default theme activity_bg fill"
        );
        assert!(
            svg.contains("Do something"),
            "action text must appear in SVG"
        );
        // Java: text is manually centered (no text-anchor attribute)
        assert!(
            svg.contains("Do something"),
            "action text must be centered in box"
        );
    }

    #[test]
    fn test_action_multiline_text() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Action,
            30.0,
            40.0,
            160.0,
            52.0,
            "Line one\nLine two",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        // Java: each line is a separate <text> element
        assert!(svg.contains("Line one"), "first line must appear");
        assert!(svg.contains("Line two"), "second line must appear");
        assert!(
            svg.matches("font-size=\"12\"").count() >= 2,
            "two lines must produce two <text> elements"
        );
    }

    #[test]
    fn test_diamond_node() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Diamond,
            60.0,
            50.0,
            40.0,
            40.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains("<polygon"),
            "diamond must be rendered as polygon"
        );
        assert!(
            svg.contains(r##"fill="#F1F1F1""##),
            "diamond must use ENTITY_BG"
        );
        assert!(
            svg.contains("stroke:#181818"),
            "diamond must use BORDER_COLOR"
        );
        assert!(svg.contains("80,50"), "diamond top vertex");
        assert!(svg.contains("100,70"), "diamond right vertex");
        assert!(svg.contains("80,90"), "diamond bottom vertex");
        assert!(svg.contains("60,70"), "diamond left vertex");
    }

    #[test]
    fn test_fork_bar() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::ForkBar,
            40.0,
            60.0,
            120.0,
            6.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(&format!(r#"fill="{FORK_FILL}""#)),
            "fork bar must be black filled"
        );
        assert!(
            svg.contains(r#"stroke="none""#),
            "fork bar must have no stroke"
        );
    }

    #[test]
    fn test_note_node() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Note {
                position: NotePositionLayout::Right,
                mode: ActivityNoteModeLayout::Grouped,
            },
            10.0,
            20.0,
            100.0,
            40.0,
            "Remember this",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(&format!(r#"fill="{NOTE_BG}""#)),
            "note must use yellow background"
        );
        assert!(svg.contains(">Remember this<"), "note text must appear");
        assert!(svg.contains("<path"), "note must use <path> elements");
        assert!(
            svg.contains("stroke-width:0.5;"),
            "note must have stroke-width 0.5"
        );
    }

    #[test]
    fn test_single_attached_note_renders_linked_opale() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Note {
                position: NotePositionLayout::Right,
                mode: ActivityNoteModeLayout::Single,
            },
            10.0,
            20.0,
            100.0,
            40.0,
            "Linked",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(r#"L-10,40"#),
            "single attached note must render the left callout notch"
        );
        assert!(svg.contains(">Linked<"), "linked note text must appear");
    }

    #[test]
    fn test_edge_with_inline_arrow() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.edges.push(ActivityEdgeLayout {
            from_index: 0,
            to_index: 1,
            label: String::new(),
            points: vec![(100.0, 30.0), (100.0, 80.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains("<polygon"),
            "edge must have inline polygon arrowhead"
        );
        assert!(svg.contains("stroke:#181818"), "edge must use BORDER_COLOR");
        assert!(svg.contains("<line "), "2-point edge must use <line>");
        assert!(
            !svg.contains("marker-end"),
            "edges must use inline polygon, not marker-end"
        );
    }

    #[test]
    fn test_edge_with_label() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.edges.push(ActivityEdgeLayout {
            from_index: 0,
            to_index: 1,
            label: "yes".to_string(),
            points: vec![(100.0, 30.0), (100.0, 80.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(svg.contains("yes"), "edge label must appear in SVG");
    }

    #[test]
    fn test_multi_segment_edge() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.edges.push(ActivityEdgeLayout {
            from_index: 0,
            to_index: 1,
            label: String::new(),
            points: vec![(50.0, 20.0), (50.0, 50.0), (100.0, 50.0), (100.0, 80.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        let line_count = svg.matches("<line ").count();
        assert!(
            line_count >= 3,
            "4-point edge must produce at least 3 line segments, got {line_count}"
        );
        assert!(
            svg.contains("<polygon"),
            "multi-segment edge must have inline polygon arrowhead"
        );
    }

    #[test]
    fn test_swimlane_headers() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.width = 400.0;
        layout.height = 300.0;
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane A".to_string(),
            x: 0.0,
            width: 200.0,
        });
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane B".to_string(),
            x: 200.0,
            width: 200.0,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(svg.contains("Lane A"), "swimlane A header must appear");
        assert!(svg.contains("Lane B"), "swimlane B header must appear");
        assert!(
            svg.contains("stroke:#000000"),
            "swimlane must have #000000 border"
        );
        assert!(
            svg.contains("stroke-width:1.5;"),
            "swimlane lines must have stroke-width 1.5"
        );
        // Divider lines extend to content bottom (max node y+h), which is 0
        // for an empty layout.  Just verify they exist.
        assert!(
            svg.contains("y2="),
            "swimlane dividers must have y2 attribute"
        );
    }

    #[test]
    fn test_xml_escape_in_action() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Action,
            10.0,
            10.0,
            160.0,
            36.0,
            "A & B < C",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains("A &amp; B &lt; C"),
            "special characters must be XML-escaped"
        );
    }

    #[test]
    fn test_end_node_same_as_stop() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::End,
            90.0,
            80.0,
            22.0,
            22.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert_eq!(
            svg.matches("<ellipse").count(),
            2,
            "End node must produce two ellipses like Stop"
        );
    }

    #[test]
    fn test_swimlane_text_headers() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.width = 400.0;
        layout.height = 300.0;
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane X".to_string(),
            x: 0.0,
            width: 200.0,
        });
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane Y".to_string(),
            x: 200.0,
            width: 200.0,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(r#"font-size="18""#),
            "swimlane headers must use font-size 18"
        );
        assert!(
            svg.contains(r#"x1="400""#),
            "right border of last swimlane must be present"
        );
    }

    #[test]
    fn test_swimlane_headers_are_centered_on_lane_content() {
        let mut diagram = empty_diagram();
        diagram.swimlanes = vec!["AA".to_string(), "BB".to_string()];
        diagram.events = vec![
            crate::model::activity::ActivityEvent::Swimlane {
                name: "AA".to_string(),
            },
            crate::model::activity::ActivityEvent::Action {
                text: "first".to_string(),
            },
            crate::model::activity::ActivityEvent::Swimlane {
                name: "BB".to_string(),
            },
            crate::model::activity::ActivityEvent::Action {
                text: "second".to_string(),
            },
        ];

        let mut layout = empty_layout();
        layout.width = 420.0;
        layout.height = 240.0;
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "AA".to_string(),
            x: 20.0,
            width: 90.0,
        });
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "BB".to_string(),
            x: 220.0,
            width: 70.0,
        });
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Action,
            40.0,
            80.0,
            80.0,
            36.0,
            "first",
        ));
        layout.nodes.push(make_node(
            1,
            ActivityNodeKindLayout::Action,
            260.0,
            80.0,
            60.0,
            36.0,
            "second",
        ));

        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");

        let lane_a_title_width = font_metrics::text_width("AA", "SansSerif", 18.0, false, false);
        let lane_b_title_width = font_metrics::text_width("BB", "SansSerif", 18.0, false, false);
        let lane_a_x = fmt_coord(20.0 + 5.0 + ((90.0 - 10.0) - lane_a_title_width) / 2.0);
        let lane_b_x = fmt_coord(220.0 + 5.0 + ((70.0 - 10.0) - lane_b_title_width) / 2.0);

        assert!(
            svg.contains(&format!(r#"x="{lane_a_x}""#)),
            "lane A header should be centered on `sw.width - 10`"
        );
        assert!(
            svg.contains(&format!(r#"x="{lane_b_x}""#)),
            "lane B header should be centered on `sw.width - 10`"
        );
    }

    #[test]
    fn test_cross_lane_multi_segment_edge() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.width = 400.0;
        layout.height = 300.0;
        layout.edges.push(ActivityEdgeLayout {
            from_index: 0,
            to_index: 1,
            label: String::new(),
            points: vec![(100.0, 50.0), (100.0, 80.0), (300.0, 80.0), (300.0, 110.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        let line_count = svg.matches("<line ").count();
        assert!(
            line_count >= 3,
            "4-point cross-lane edge must produce at least 3 line segments"
        );
        assert!(
            svg.contains("<polygon"),
            "cross-lane edge must have inline polygon arrowhead"
        );
    }

    #[test]
    fn test_swimlane_edges_render_same_lane_before_cross_lane() {
        use crate::model::activity::ActivityEvent;

        let diagram = ActivityDiagram {
            events: vec![
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Start,
                ActivityEvent::Action { text: "A1".into() },
                ActivityEvent::Swimlane {
                    name: "Lane B".into(),
                },
                ActivityEvent::Action { text: "B1".into() },
                ActivityEvent::Swimlane {
                    name: "Lane A".into(),
                },
                ActivityEvent::Action { text: "A2".into() },
                ActivityEvent::Stop,
            ],
            swimlanes: vec!["Lane A".into(), "Lane B".into()],
            direction: Default::default(),
            note_max_width: None,
            is_old_style: false,
            old_graph: None,
        };

        let mut layout = empty_layout();
        layout.width = 320.0;
        layout.height = 260.0;
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane A".to_string(),
            x: 20.0,
            width: 100.0,
        });
        layout.swimlane_layouts.push(SwimlaneLayout {
            name: "Lane B".to_string(),
            x: 140.0,
            width: 100.0,
        });
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Start,
            50.0,
            20.0,
            20.0,
            20.0,
            "",
        ));
        layout.nodes.push(make_node(
            1,
            ActivityNodeKindLayout::Action,
            35.0,
            60.0,
            50.0,
            20.0,
            "A1",
        ));
        layout.nodes.push(make_node(
            2,
            ActivityNodeKindLayout::Action,
            155.0,
            100.0,
            50.0,
            20.0,
            "B1",
        ));
        layout.nodes.push(make_node(
            3,
            ActivityNodeKindLayout::Action,
            35.0,
            140.0,
            50.0,
            20.0,
            "A2",
        ));
        layout.nodes.push(make_node(
            4,
            ActivityNodeKindLayout::Stop,
            49.0,
            180.0,
            22.0,
            22.0,
            "",
        ));
        layout.edges.push(ActivityEdgeLayout {
            from_index: 0,
            to_index: 1,
            label: String::new(),
            points: vec![(60.0, 40.0), (60.0, 60.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        layout.edges.push(ActivityEdgeLayout {
            from_index: 1,
            to_index: 2,
            label: String::new(),
            points: vec![(60.0, 80.0), (60.0, 85.0), (180.0, 85.0), (180.0, 100.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        layout.edges.push(ActivityEdgeLayout {
            from_index: 2,
            to_index: 3,
            label: String::new(),
            points: vec![(180.0, 120.0), (180.0, 125.0), (60.0, 125.0), (60.0, 140.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });
        layout.edges.push(ActivityEdgeLayout {
            from_index: 3,
            to_index: 4,
            label: String::new(),
            points: vec![(60.0, 160.0), (60.0, 180.0)],
            kind: ActivityEdgeKindLayout::Normal,
        });

        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");

        let same_lane_tail = svg
            .find(r#"x1="60" x2="60" y1="160" y2="180""#)
            .expect("missing same-lane edge");
        let cross_lane_head = svg
            .find(r#"x1="60" x2="60" y1="80" y2="85""#)
            .expect("missing cross-lane edge");
        assert!(
            same_lane_tail < cross_lane_head,
            "same-lane edges should render before cross-lane edges"
        );
    }

    #[test]
    fn test_fmt_coord_in_output() {
        let diagram = empty_diagram();
        let mut layout = empty_layout();
        layout.nodes.push(make_node(
            0,
            ActivityNodeKindLayout::Start,
            90.0,
            10.0,
            20.0,
            20.0,
            "",
        ));
        let (svg, _raw_dim) = render_activity(&diagram, &layout, &SkinParams::default(), None)
            .expect("render failed");
        assert!(
            svg.contains(r#"cx="100""#),
            "fmt_coord must strip trailing .0"
        );
        assert!(
            svg.contains(r#"cy="20""#),
            "fmt_coord must strip trailing .0"
        );
    }
}
