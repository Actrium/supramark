//! C4 layout — bound-packing rows and rectangle-edge intersections.
//!
//! Mirrors upstream `c4Renderer.js` (mermaid@11.14.0) line-for-line.
//!
//! Each shape gets per-text-block width/height computed from DejaVu
//! Sans/SansBold (the same font tables the upstream jsdom shim feeds
//! its `getBBox` calls). All measurements are rounded with
//! `Math.round` to match `calculateTextDimensions`'s line-height
//! rounding. Boundary rectangles are positioned so they enclose all
//! their direct children plus an inner margin.

use crate::font_metrics::{line_height as fm_line_height, text_width as fm_text_width};
use crate::model::c4::{C4Boundary, C4Diagram, C4Rel, C4Shape};

// ── C4 default config (mirrors config.schema.yaml C4DiagramConfig) ─────

#[derive(Debug, Clone)]
pub struct C4Conf {
    pub diagram_margin_x: f64,
    pub diagram_margin_y: f64,
    pub c4_shape_margin: f64,
    pub c4_shape_padding: f64,
    pub width: f64,
    pub height: f64,
    pub box_margin: f64,
    pub c4_shape_in_row: u32,
    pub c4_boundary_in_row: u32,
    pub next_line_padding_x: f64,
    pub wrap: bool,
    pub wrap_padding: f64,
    pub message_font_size: f64,
    pub message_font_family: String,
    pub boundary_font_size: f64,
    pub boundary_font_family: String,
    pub default_font_size: f64,
    pub default_font_family: String,
}

impl Default for C4Conf {
    fn default() -> Self {
        Self {
            diagram_margin_x: 50.0,
            diagram_margin_y: 10.0,
            c4_shape_margin: 50.0,
            c4_shape_padding: 20.0,
            width: 216.0,
            height: 60.0,
            box_margin: 10.0,
            c4_shape_in_row: 4,
            c4_boundary_in_row: 2,
            next_line_padding_x: 0.0,
            wrap: true,
            wrap_padding: 10.0,
            message_font_size: 12.0,
            message_font_family: "\"Open Sans\", sans-serif".to_string(),
            boundary_font_size: 14.0,
            boundary_font_family: "\"Open Sans\", sans-serif".to_string(),
            default_font_size: 14.0,
            default_font_family: "\"Open Sans\", sans-serif".to_string(),
        }
    }
}

// ── Per-shape text block (label / type / techn / descr) ────────────────

#[derive(Debug, Clone, Default)]
pub struct TextBlock {
    pub text: String,
    pub width: f64,
    pub height: f64,
    pub y_offset: f64,
    pub text_lines: u32,
}

// ── Laid-out shape (one rect + 4 text blocks + optional sprite) ────────

#[derive(Debug, Clone)]
pub struct LaidShape {
    pub idx: usize, // index into C4Diagram.shapes
    pub kind: String,
    pub alias: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub label: TextBlock,
    pub typ: TextBlock,         // ['type'] in upstream
    pub techn: TextBlock,
    pub descr: TextBlock,
    /// `<<typeC4Shape>>` text block (italic, smaller font).
    pub type_c4: TextBlock,
    pub image_y: f64,
    pub image_width: f64,
    pub image_height: f64,
    pub bg_color: String,
    pub border_color: String,
    pub font_color: String,
    pub has_techn: bool,
    pub has_type: bool,
    pub has_descr: bool,
}

#[derive(Debug, Clone)]
pub struct LaidBoundary {
    pub idx: usize, // index into C4Diagram.boundaries
    pub alias: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label: TextBlock,
    pub typ: TextBlock,
    pub descr: TextBlock,
    pub has_type: bool,
    pub has_descr: bool,
    pub border_color: String,
    pub bg_color: String, // "none" if not specified
    pub font_color: String,
    pub stroke_dasharray: bool, // false if Deployment_Node
}

#[derive(Debug, Clone)]
pub struct LaidRel {
    pub idx: usize,
    pub from_alias: String,
    pub to_alias: String,
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub label: TextBlock,
    pub techn: TextBlock,
    pub has_techn: bool,
    pub rel_type: String,
    pub text_color: String,
    pub line_color: String,
    pub offset_x: f64,
    pub offset_y: f64,
    /// Index of this rel among rels (for the curve test: `i == 0` is line, else path).
    pub render_order: usize,
}

/// Element emitted by the layout in draw order.
#[derive(Debug, Clone)]
pub enum LaidElement {
    Shape(LaidShape),
    Boundary(LaidBoundary),
}

#[derive(Debug, Clone)]
pub struct C4Layout {
    pub conf: C4Conf,
    pub shapes: Vec<LaidShape>,
    pub boundaries: Vec<LaidBoundary>,
    pub rels: Vec<LaidRel>,
    /// Shapes and boundaries in emission order (matches upstream's
    /// drawC4ShapeArray/drawBoundary call sequence inside
    /// drawInsideBoundary).
    pub elements: Vec<LaidElement>,
    /// SVG viewBox: `(min_x, min_y, width, height)`
    pub view_min_x: f64,
    pub view_min_y: f64,
    pub view_width: f64,
    pub view_height: f64,
    /// SVG attribute `width` (max-width). Equals `view_width`.
    pub svg_width: f64,
    pub svg_height: f64,
    /// True if a title was emitted (adds 60px).
    pub has_title: bool,
    pub title: String,
    pub title_x: f64,
    pub title_y: f64,
    /// Diagram subtype string for `aria-roledescription`.
    pub aria_roledescr: String,
}

// ── Public entry point ─────────────────────────────────────────────────

pub fn layout(diag: &C4Diagram) -> C4Layout {
    let mut conf = C4Conf::default();
    if let Some(n) = diag.c4_shape_in_row {
        conf.c4_shape_in_row = n;
    }
    if let Some(n) = diag.c4_boundary_in_row {
        conf.c4_boundary_in_row = n;
    }

    let mut state = LayoutState {
        diag,
        conf: conf.clone(),
        shapes_out: Vec::new(),
        boundaries_out: Vec::new(),
        elements_out: Vec::new(),
        rels_out: Vec::new(),
        global_max_x: conf.diagram_margin_x,
        global_max_y: conf.diagram_margin_y,
    };

    // Top-level "screen" bounds.
    let mut screen = Bounds::new();
    screen.set_data(
        conf.diagram_margin_x,
        conf.diagram_margin_x,
        conf.diagram_margin_y,
        conf.diagram_margin_y,
    );
    // Match upstream: screen.data.widthLimit = screen.availWidth.
    // jsdom's Screen reports availWidth=0 by default, and the
    // generate_ref shim keeps that value (its `if (!globalThis.screen)`
    // guard never fires because jsdom preserves the original Screen
    // object). With widthLimit=0, every shape's _stopx >= 0 triggers
    // the wrap branch, which is what makes c4 fixtures stack vertically.
    screen.data.width_limit = Some(0.0);

    // Top-level: just the synthetic 'global' boundary, which has
    // parent_boundary == "" in our model. Upstream's getBoundaries('')
    // returns global only, then recursion handles real boundaries.
    let top_level: Vec<usize> = diag
        .boundaries
        .iter()
        .enumerate()
        .filter(|(_, b)| b.parent_boundary.is_empty())
        .map(|(i, _)| i)
        .collect();

    state.draw_inside_boundary("", &mut screen, &top_level);

    // Compute viewBox.
    screen.data.stopx = Some(state.global_max_x);
    screen.data.stopy = Some(state.global_max_y);

    let box_height = screen.data.stopy.unwrap_or(0.0) - screen.data.starty.unwrap_or(0.0);
    let height = box_height + 2.0 * conf.diagram_margin_y;

    let box_width = screen.data.stopx.unwrap_or(0.0) - screen.data.startx.unwrap_or(0.0);
    let width = box_width + 2.0 * conf.diagram_margin_x;

    let title = diag.meta.title.clone().unwrap_or_default();
    let has_title = !title.is_empty();
    let title_x = (screen.data.stopx.unwrap_or(0.0) - screen.data.startx.unwrap_or(0.0)) / 2.0
        - 4.0 * conf.diagram_margin_x;
    let title_y = screen.data.starty.unwrap_or(0.0) + conf.diagram_margin_y;

    let extra_for_title = if has_title { 60.0 } else { 0.0 };
    let view_min_x = screen.data.startx.unwrap_or(0.0) - conf.diagram_margin_x;
    let view_min_y = -(conf.diagram_margin_y + extra_for_title);
    let view_width = width;
    let view_height = height + extra_for_title;

    // Compute relationship intersection points after all shapes are placed.
    let mut rels_out: Vec<LaidRel> = Vec::with_capacity(diag.rels.len());
    let mut order = 0usize;
    for (i, rel) in diag.rels.iter().enumerate() {
        if let (Some(from), Some(to)) = (
            shape_lookup(&state.shapes_out, &rel.from),
            shape_lookup(&state.shapes_out, &rel.to),
        ) {
            let (sp, ep) = intersect_points(from, to);
            // Compute label/techn dims using messageFont.
            let mf_fam = conf.message_font_family.clone();
            let mf_sz = conf.message_font_size;
            let dyn_prefix = if diag.subtype == crate::model::c4::C4Subtype::Dynamic {
                format!("{}: ", i + 1)
            } else {
                String::new()
            };
            let mut label_block = TextBlock::default();
            label_block.text = format!("{}{}", dyn_prefix, rel.label.text);
            // calcC4ShapeTextWH for label: not wrapped (textLimitWidth = own width).
            // Upstream: textLimitWidth = calculateTextWidth(rel.label.text, relConf).
            // With wrap off (default rel doesn't wrap): single line, width = textWidth.
            let lab_w = round_w(fm_text_width(&label_block.text, &mf_fam, mf_sz, false, false));
            let lab_h = round_w(fm_line_height(&mf_fam, mf_sz, false, false));
            label_block.width = lab_w;
            label_block.height = lab_h;
            label_block.text_lines = 1;
            let mut techn_block = TextBlock::default();
            let has_techn = !rel.techn.text.is_empty();
            if has_techn {
                techn_block.text = rel.techn.text.clone();
                let tw = round_w(fm_text_width(&techn_block.text, &mf_fam, mf_sz, false, false));
                let th = round_w(fm_line_height(&mf_fam, mf_sz, false, false));
                techn_block.width = tw;
                techn_block.height = th;
                techn_block.text_lines = 1;
            }
            // Style overrides.
            let text_color = rel.text_color.clone().unwrap_or_else(|| "#444444".into());
            let line_color = rel.line_color.clone().unwrap_or_else(|| "#444444".into());
            let offset_x = rel.offset_x.map(|v| v as f64).unwrap_or(0.0);
            let offset_y = rel.offset_y.map(|v| v as f64).unwrap_or(0.0);
            rels_out.push(LaidRel {
                idx: i,
                from_alias: rel.from.clone(),
                to_alias: rel.to.clone(),
                start_x: sp.0,
                start_y: sp.1,
                end_x: ep.0,
                end_y: ep.1,
                label: label_block,
                techn: techn_block,
                has_techn,
                rel_type: rel.rel_type.clone(),
                text_color,
                line_color,
                offset_x,
                offset_y,
                render_order: order,
            });
            order = if order == 0 { usize::MAX } else { 0 };
        }
    }
    // Re-do the i==0 vs i==-1 logic exactly as upstream:
    //   if i === 0: line; i = -1; else: path.
    // We model that with `render_order`: 0 → line, 1.. → path.
    // Re-stamp render_order based on the order they got laid out.
    for (k, r) in rels_out.iter_mut().enumerate() {
        r.render_order = k;
    }

    let aria_roledescr = "c4".to_string();

    C4Layout {
        conf,
        shapes: state.shapes_out,
        boundaries: state.boundaries_out,
        elements: state.elements_out,
        rels: rels_out,
        view_min_x,
        view_min_y,
        view_width,
        view_height,
        svg_width: view_width,
        svg_height: view_height,
        has_title,
        title,
        title_x,
        title_y,
        aria_roledescr,
    }
}

// ── Internal types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct BoundsData {
    startx: Option<f64>,
    stopx: Option<f64>,
    starty: Option<f64>,
    stopy: Option<f64>,
    width_limit: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct NextData {
    startx: Option<f64>,
    stopx: Option<f64>,
    starty: Option<f64>,
    stopy: Option<f64>,
    cnt: u32,
}

#[derive(Debug, Clone)]
struct Bounds {
    name: String,
    data: BoundsData,
    next_data: NextData,
}

impl Bounds {
    fn new() -> Self {
        Self {
            name: String::new(),
            data: BoundsData::default(),
            next_data: NextData::default(),
        }
    }
    fn set_data(&mut self, sx: f64, ex: f64, sy: f64, ey: f64) {
        self.data.startx = Some(sx);
        self.data.stopx = Some(ex);
        self.data.starty = Some(sy);
        self.data.stopy = Some(ey);
        self.next_data.startx = Some(sx);
        self.next_data.stopx = Some(ex);
        self.next_data.starty = Some(sy);
        self.next_data.stopy = Some(ey);
    }
    fn bump_last_margin(&mut self, m: f64) {
        if let Some(v) = self.data.stopx.as_mut() {
            *v += m;
        }
        if let Some(v) = self.data.stopy.as_mut() {
            *v += m;
        }
    }
    /// `insert(c4Shape)` mirroring upstream Bounds.insert.
    fn insert(
        &mut self,
        width: f64,
        height: f64,
        margin: f64,
        c4_shape_in_row: u32,
        next_line_padding_x: f64,
    ) -> (f64, f64) {
        self.next_data.cnt += 1;
        let next_startx = self.next_data.startx.unwrap_or(0.0);
        let next_stopx = self.next_data.stopx.unwrap_or(0.0);
        let next_starty = self.next_data.starty.unwrap_or(0.0);
        let next_stopy = self.next_data.stopy.unwrap_or(0.0);
        let mut sx = if (next_startx - next_stopx).abs() < f64::EPSILON {
            next_stopx + margin
        } else {
            next_stopx + margin * 2.0
        };
        let mut ex = sx + width;
        let mut sy = next_starty + margin * 2.0;
        let mut ey = sy + height;
        let width_limit = self.data.width_limit.unwrap_or(f64::INFINITY);
        if sx >= width_limit || ex >= width_limit || self.next_data.cnt > c4_shape_in_row {
            sx = next_startx + margin + next_line_padding_x;
            sy = next_stopy + margin * 2.0;
            self.next_data.stopx = Some(sx + width);
            self.next_data.starty = Some(next_stopy);
            self.next_data.stopy = Some(sy + height);
            ex = sx + width;
            ey = sy + height;
            self.next_data.cnt = 1;
        }
        // Update min/max trackers on data.
        upd_min(&mut self.data.startx, sx);
        upd_min(&mut self.data.starty, sy);
        upd_max(&mut self.data.stopx, ex);
        upd_max(&mut self.data.stopy, ey);
        upd_min(&mut self.next_data.startx, sx);
        upd_min(&mut self.next_data.starty, sy);
        upd_max(&mut self.next_data.stopx, ex);
        upd_max(&mut self.next_data.stopy, ey);
        (sx, sy)
    }
}

fn upd_min(slot: &mut Option<f64>, v: f64) {
    *slot = Some(match slot {
        Some(prev) => prev.min(v),
        None => v,
    });
}
fn upd_max(slot: &mut Option<f64>, v: f64) {
    *slot = Some(match slot {
        Some(prev) => prev.max(v),
        None => v,
    });
}

struct LayoutState<'a> {
    diag: &'a C4Diagram,
    conf: C4Conf,
    shapes_out: Vec<LaidShape>,
    boundaries_out: Vec<LaidBoundary>,
    elements_out: Vec<LaidElement>,
    rels_out: Vec<LaidRel>,
    global_max_x: f64,
    global_max_y: f64,
}

impl<'a> LayoutState<'a> {
    fn draw_inside_boundary(
        &mut self,
        _parent_alias: &str,
        parent_bounds: &mut Bounds,
        current_boundaries: &[usize],
    ) {
        let mut current_bounds = Bounds::new();
        // currentBounds.data.widthLimit = parentBounds.data.widthLimit / Math.min(c4BoundaryInRow, currentBoundaries.length).
        let n = current_boundaries.len() as f64;
        let limit = parent_bounds.data.width_limit.unwrap_or(1024.0)
            / (self.conf.c4_boundary_in_row as f64).min(n.max(1.0));
        current_bounds.data.width_limit = Some(limit);

        for (i_pos, &b_idx) in current_boundaries.iter().enumerate() {
            let bnd = &self.diag.boundaries[b_idx];
            // Calculate boundary text dims (label, type, descr).
            let mut y = 0.0_f64;
            let image_w = 0.0;
            let image_h = 0.0;
            let _ = (image_w, image_h);
            // Boundary label (boundaryFont, fontSize +2, bold).
            let bf_fam = self.conf.boundaryFontFamily();
            let bf_sz = self.conf.boundary_font_size;
            let label_text = bnd.label.text.clone();
            let (label_w, label_h, _) =
                measure_lines(&label_text, &bf_fam, bf_sz + 2.0, false, false);
            let label_y = y + 8.0;
            y = label_y + label_h;
            // Type
            let mut typ_block = TextBlock::default();
            let has_type = !bnd.b_type.text.is_empty();
            if has_type {
                let tt = format!("[{}]", bnd.b_type.text);
                let (tw, th, _) = measure_lines(&tt, &bf_fam, bf_sz, false, false);
                typ_block.text = tt;
                typ_block.width = tw;
                typ_block.height = th;
                typ_block.text_lines = 1;
                typ_block.y_offset = y + 5.0;
                y = typ_block.y_offset + th;
            }
            // Descr
            let mut descr_block = TextBlock::default();
            let has_descr = !bnd.descr.text.is_empty();
            if has_descr {
                let dt = bnd.descr.text.clone();
                let (dw, dh, _) = measure_lines(&dt, &bf_fam, bf_sz - 2.0, false, false);
                descr_block.text = dt;
                descr_block.width = dw;
                descr_block.height = dh;
                descr_block.text_lines = 1;
                descr_block.y_offset = y + 20.0;
                y = descr_block.y_offset + dh;
            }
            let _ = y;

            // Decide where this boundary's currentBounds starts.
            if i_pos == 0 || (i_pos as u32) % self.conf.c4_boundary_in_row == 0 {
                let x0 = parent_bounds.data.startx.unwrap_or(0.0) + self.conf.diagram_margin_x;
                let y0 = parent_bounds.data.stopy.unwrap_or(0.0) + self.conf.diagram_margin_y + y;
                current_bounds.set_data(x0, x0, y0, y0);
            } else {
                let x0 = if (current_bounds.data.stopx.unwrap_or(0.0)
                    - current_bounds.data.startx.unwrap_or(0.0))
                    .abs()
                    > f64::EPSILON
                {
                    current_bounds.data.stopx.unwrap_or(0.0) + self.conf.diagram_margin_x
                } else {
                    current_bounds.data.startx.unwrap_or(0.0)
                };
                let y0 = current_bounds.data.starty.unwrap_or(0.0);
                current_bounds.set_data(x0, x0, y0, y0);
            }
            current_bounds.name = bnd.alias.clone();

            // Place direct shape children of this boundary.
            let shape_indices: Vec<usize> = self
                .diag
                .shapes
                .iter()
                .enumerate()
                .filter(|(_, s)| s.parent_boundary == bnd.alias)
                .map(|(i, _)| i)
                .collect();
            if !shape_indices.is_empty() {
                self.draw_shapes(&mut current_bounds, &shape_indices);
            }

            // Recurse into nested boundaries.
            let nested: Vec<usize> = self
                .diag
                .boundaries
                .iter()
                .enumerate()
                .filter(|(_, b)| b.parent_boundary == bnd.alias)
                .map(|(i, _)| i)
                .collect();
            if !nested.is_empty() {
                self.draw_inside_boundary(&bnd.alias, &mut current_bounds, &nested);
            }

            // Now that children are placed, draw_boundary computes
            // boundary x/y/w/h from currentBounds.data.
            if bnd.alias != "global" {
                let bx = current_bounds.data.startx.unwrap_or(0.0);
                let by = current_bounds.data.starty.unwrap_or(0.0);
                let bw = current_bounds.data.stopx.unwrap_or(0.0) - bx;
                let bh = current_bounds.data.stopy.unwrap_or(0.0) - by;
                // Build final y offsets for label/type/descr relative to
                // boundary.y. Mirrors upstream:
                //   label.Y = 0 + 8
                //   type.Y  = label.Y + label.height + 5
                //   descr.Y = (last bottom) + 20
                let mut yy = 0.0_f64;
                let mut label_block = TextBlock::default();
                label_block.text = label_text;
                label_block.width = label_w;
                label_block.height = label_h;
                label_block.text_lines = 1;
                label_block.y_offset = yy + 8.0;
                yy = label_block.y_offset + label_block.height;
                let mut typ_final = TextBlock::default();
                if has_type {
                    typ_final.text = typ_block.text.clone();
                    typ_final.width = typ_block.width;
                    typ_final.height = typ_block.height;
                    typ_final.text_lines = 1;
                    typ_final.y_offset = yy + 5.0;
                    yy = typ_final.y_offset + typ_final.height;
                }
                let mut descr_final = TextBlock::default();
                if has_descr {
                    descr_final.text = descr_block.text.clone();
                    descr_final.width = descr_block.width;
                    descr_final.height = descr_block.height;
                    descr_final.text_lines = 1;
                    descr_final.y_offset = yy + 20.0;
                }
                let stroke_dash = bnd.node_type.is_none();
                let border = bnd
                    .border_color
                    .clone()
                    .unwrap_or_else(|| "#444444".to_string());
                let bg = bnd.bg_color.clone().unwrap_or_else(|| "none".to_string());
                let font_color = bnd.font_color.clone().unwrap_or_else(|| "black".to_string());
                let laid = LaidBoundary {
                    idx: b_idx,
                    alias: bnd.alias.clone(),
                    x: bx,
                    y: by,
                    width: bw,
                    height: bh,
                    label: label_block,
                    typ: typ_final,
                    descr: descr_final,
                    has_type,
                    has_descr,
                    border_color: border,
                    bg_color: bg,
                    font_color,
                    stroke_dasharray: stroke_dash,
                };
                self.boundaries_out.push(laid.clone());
                self.elements_out.push(LaidElement::Boundary(laid));
            }

            // parentBounds.data.stopy/x = max(currentBounds.data.* + c4ShapeMargin, parentBounds.data.*)
            let cb_stopy = current_bounds.data.stopy.unwrap_or(0.0);
            let cb_stopx = current_bounds.data.stopx.unwrap_or(0.0);
            let pby = parent_bounds.data.stopy.unwrap_or(0.0);
            let pbx = parent_bounds.data.stopx.unwrap_or(0.0);
            parent_bounds.data.stopy = Some((cb_stopy + self.conf.c4_shape_margin).max(pby));
            parent_bounds.data.stopx = Some((cb_stopx + self.conf.c4_shape_margin).max(pbx));
            self.global_max_x = self.global_max_x.max(parent_bounds.data.stopx.unwrap_or(0.0));
            self.global_max_y = self.global_max_y.max(parent_bounds.data.stopy.unwrap_or(0.0));
        }
    }

    fn draw_shapes(&mut self, current_bounds: &mut Bounds, shape_indices: &[usize]) {
        for &s_idx in shape_indices {
            let s = &self.diag.shapes[s_idx];
            let kind = s.type_c4_shape.clone();
            let conf = self.conf.clone();

            // ── typeC4Shape («kind»)
            let cf_fam = conf.shape_font_family(&kind);
            let cf_sz = conf.shape_font_size(&kind);
            // upstream: c4ShapeTypeConf.fontSize = c4ShapeTypeConf.fontSize - 2;
            // measured text is `'«' + kind + '»'`
            let type_meas_text = format!("\u{ab}{}\u{bb}", kind);
            let type_w = round_w(fm_text_width(&type_meas_text, &cf_fam, cf_sz - 2.0, false, false));
            let type_h = (cf_sz - 2.0) + 2.0; // upstream: c4ShapeTypeConf.fontSize + 2

            let mut type_c4 = TextBlock::default();
            type_c4.text = format!("<<{}>>", kind);
            type_c4.width = type_w;
            type_c4.height = type_h;
            type_c4.text_lines = 1;
            type_c4.y_offset = conf.c4_shape_padding;
            let mut yy = type_c4.y_offset + type_h - 4.0;

            // ── Image (sprite) for person/external_person.
            let mut image_y = 0.0;
            let mut image_w = 0.0;
            let mut image_h = 0.0;
            let needs_image = matches!(kind.as_str(), "person" | "external_person")
                || s.sprite.is_some();
            if needs_image {
                image_w = 48.0;
                image_h = 48.0;
                image_y = yy;
                yy = image_y + image_h;
            }

            // ── Label
            // c4Shape.wrap is set at parse time from autoWrap()=false, so
            // labels are NEVER wrapped — measured per-line at full width.
            let _text_limit_width = conf.width - conf.c4_shape_padding * 2.0;
            let label_font_sz = cf_sz + 2.0;
            let (label_w, label_h, label_lines_n) =
                measure_lines(&s.label.text, &cf_fam, label_font_sz, true, false);
            let mut label = TextBlock::default();
            label.text = s.label.text.clone();
            label.text_lines = label_lines_n;
            label.width = label_w;
            label.height = label_h;
            label.y_offset = yy + 8.0;
            yy = label.y_offset + label.height;

            // ── Type [type]
            let mut typ = TextBlock::default();
            let has_type = !s.descr.text.is_empty() || (kind.starts_with("container") || kind.starts_with("component") || kind.contains("container") || kind.contains("component"));
            // Upstream: prefers c4Shape.type if set, else techn; for our parser
            // Person/System fill `descr` only; Container/Component fill `techn`
            // and `descr`. The DOM emits type before descr only when techn is
            // empty AND we have a "type" field. Our model collapses:
            //   - For person/system: techn="" descr=descr → type="" not used.
            //   - For container/component: techn="techn" descr=descr → use techn.
            // So we render either techn or none on this slot.
            let _ = has_type;
            // Techn slot.
            // Upstream bug: `c4ShapeFont(conf, c4Shape.techn.text)` uses
            // the bracketed techn TEXT as the font key, which never
            // resolves; calcC4ShapeTextWH then sees undefined fields and
            // calculateTextDimensions falls back to {fontSize:12,
            // fontFamily:'Arial'}. So techn is measured at 12px Arial
            // (which the DejaVu shim treats as sans).
            let mut techn = TextBlock::default();
            let has_techn = !s.techn.text.is_empty();
            if has_techn {
                let t = format!("[{}]", s.techn.text);
                let (tw, th, tn) = measure_lines(&t, "Arial", 12.0, false, false);
                techn.text = t;
                techn.width = tw;
                techn.height = th;
                techn.text_lines = tn;
                techn.y_offset = yy + 5.0;
                yy = techn.y_offset + techn.height;
            }

            let mut rect_height = yy;
            let mut rect_width = label.width;

            // ── Descr
            let mut descr = TextBlock::default();
            let has_descr = !s.descr.text.is_empty();
            if has_descr {
                let (dw, dh, dn) = measure_lines(&s.descr.text, &cf_fam, cf_sz, false, false);
                descr.text = s.descr.text.clone();
                descr.width = dw;
                descr.height = dh;
                descr.text_lines = dn;
                descr.y_offset = yy + 20.0;
                yy = descr.y_offset + descr.height;

                rect_width = label.width.max(descr.width);
                // upstream: rectHeight = Y - c4Shape.descr.textLines * 5;
                rect_height = yy - (descr.text_lines as f64) * 5.0;
            }

            rect_width += conf.c4_shape_padding;
            let final_width = conf.width.max(rect_width);
            let final_height = conf.height.max(rect_height);
            let margin = conf.c4_shape_margin;

            // Insert into bounds.
            let (sx, sy) = current_bounds.insert(
                final_width,
                final_height,
                margin,
                conf.c4_shape_in_row,
                conf.next_line_padding_x,
            );

            // Style overrides.
            let bg = s
                .bg_color
                .clone()
                .unwrap_or_else(|| default_bg_color(&kind).to_string());
            let border = s
                .border_color
                .clone()
                .unwrap_or_else(|| default_border_color(&kind).to_string());
            let font_color = s.font_color.clone().unwrap_or_else(|| "#FFFFFF".to_string());

            let laid = LaidShape {
                idx: s_idx,
                kind: kind.clone(),
                alias: s.alias.clone(),
                x: sx,
                y: sy,
                width: final_width,
                height: final_height,
                margin,
                label,
                typ: TextBlock::default(),
                techn,
                descr,
                type_c4,
                image_y,
                image_width: image_w,
                image_height: image_h,
                bg_color: bg,
                border_color: border,
                font_color,
                has_techn,
                has_type: false,
                has_descr,
            };
            self.shapes_out.push(laid.clone());
            self.elements_out.push(LaidElement::Shape(laid));
        }
        current_bounds.bump_last_margin(self.conf.c4_shape_margin);
    }
}

// ── Wrap + measure ──────────────────────────────────────────────────────

/// `calcC4ShapeTextWH` port — non-wrapping branch.
///
/// Splits on line breaks, returns (max line width, total height).
/// Mirrors:
///   for line: width = max(round(textWidth(line)), width); height += round(lineHeight)
/// Upstream `calculateTextDimensions` splits the source text by
/// `lineBreakRegex = /<br\s*\/?>/gi`, processing each segment as a separate
/// line. We mirror that here so descriptions containing `<br/>` produce
/// multi-line geometry.
fn measure_lines(text: &str, family: &str, size: f64, bold: bool, italic: bool) -> (f64, f64, u32) {
    let lh = round_w(fm_line_height(family, size, bold, italic));
    if text.is_empty() {
        return (0.0, lh, 1);
    }
    let lines = split_line_breaks(text);
    let mut w: f64 = 0.0;
    let mut h: f64 = 0.0;
    for line in &lines {
        let lw = round_w(fm_text_width(line, family, size, bold, italic));
        if lw > w {
            w = lw;
        }
        h += lh;
    }
    (w, h, lines.len() as u32)
}

/// Splits a text by upstream's `lineBreakRegex = /<br\s*\/?>/gi` and by raw
/// '\n'/'\r' as well. Returns owned strings to avoid borrow lifetime issues
/// from regex iteration.
pub(crate) fn split_line_breaks(text: &str) -> Vec<String> {
    // Walk the string, accumulating segments split by any case-insensitive
    // `<br\s*/?>` token. Mirrors JS regex split semantics.
    let bytes = text.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // Try to match `<br...>` starting at i.
        if bytes[i] == b'<'
            && i + 2 < bytes.len()
            && (bytes[i + 1] == b'b' || bytes[i + 1] == b'B')
            && (bytes[i + 2] == b'r' || bytes[i + 2] == b'R')
        {
            // Skip optional whitespace.
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            // Optional self-close '/'.
            if j < bytes.len() && bytes[j] == b'/' {
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
            }
            // Closing '>'.
            if j < bytes.len() && bytes[j] == b'>' {
                out.push(std::mem::take(&mut cur));
                i = j + 1;
                continue;
            }
        }
        // Also split on raw newline characters.
        if bytes[i] == b'\n' || bytes[i] == b'\r' {
            out.push(std::mem::take(&mut cur));
            i += 1;
            continue;
        }
        // Push current byte (UTF-8 safe — copy whole char).
        let ch = text[i..].chars().next().unwrap();
        cur.push(ch);
        i += ch.len_utf8();
    }
    out.push(cur);
    out
}


// Width round mirrors Math.round used in calculateTextDimensions.
// JS `Math.round` rounds half toward +∞: 0.5 → 1, -0.5 → 0.
fn round_w(w: f64) -> f64 {
    (w + 0.5).floor()
}

fn shape_lookup<'a>(shapes: &'a [LaidShape], alias: &str) -> Option<&'a LaidShape> {
    shapes.iter().find(|s| s.alias == alias)
}

/// Rectangle-edge intersection (mirrors getIntersectPoint + getIntersectPoints).
fn intersect_points(from: &LaidShape, to: &LaidShape) -> ((f64, f64), (f64, f64)) {
    let from_rect = (from.x, from.y, from.width, from.height);
    let to_rect = (to.x, to.y, to.width, to.height);
    let to_center = (to.x + to.width / 2.0, to.y + to.height / 2.0);
    let from_center = (from.x + from.width / 2.0, from.y + from.height / 2.0);
    let sp = intersect_point(from_rect, to_center);
    let ep = intersect_point(to_rect, from_center);
    (sp, ep)
}

fn intersect_point(rect: (f64, f64, f64, f64), endpoint: (f64, f64)) -> (f64, f64) {
    let (x1, y1, w, h) = rect;
    let x2 = endpoint.0;
    let y2 = endpoint.1;
    let cx = x1 + w / 2.0;
    let cy = y1 + h / 2.0;
    let dx = (x1 - x2).abs();
    let dy = (y1 - y2).abs();
    if y1 == y2 && x1 < x2 {
        return (x1 + w, cy);
    }
    if y1 == y2 && x1 > x2 {
        return (x1, cy);
    }
    if x1 == x2 && y1 < y2 {
        return (cx, y1 + h);
    }
    if x1 == x2 && y1 > y2 {
        return (cx, y1);
    }
    let tan_dyx = dy / dx;
    let from_dyx = h / w;
    if x1 > x2 && y1 < y2 {
        if from_dyx >= tan_dyx {
            return (x1, cy + (tan_dyx * w) / 2.0);
        } else {
            return (cx - ((dx / dy) * h) / 2.0, y1 + h);
        }
    } else if x1 < x2 && y1 < y2 {
        if from_dyx >= tan_dyx {
            return (x1 + w, cy + (tan_dyx * w) / 2.0);
        } else {
            return (cx + ((dx / dy) * h) / 2.0, y1 + h);
        }
    } else if x1 < x2 && y1 > y2 {
        if from_dyx >= tan_dyx {
            return (x1 + w, cy - (tan_dyx * w) / 2.0);
        } else {
            return (cx + ((h / 2.0) * dx) / dy, y1);
        }
    } else if x1 > x2 && y1 > y2 {
        if from_dyx >= tan_dyx {
            return (x1, cy - (w / 2.0) * tan_dyx);
        } else {
            return (cx - ((h / 2.0) * dx) / dy, y1);
        }
    }
    (cx, cy)
}

// ── Default colors ──────────────────────────────────────────────────────

fn default_bg_color(kind: &str) -> &'static str {
    match kind {
        "person" => "#08427B",
        "external_person" => "#686868",
        "system" | "system_db" | "system_queue" => "#1168BD",
        "external_system" | "external_system_db" | "external_system_queue" => "#999999",
        "container" | "container_db" | "container_queue" => "#438DD5",
        "external_container" | "external_container_db" | "external_container_queue" => "#B3B3B3",
        "component" | "component_db" | "component_queue" => "#85BBF0",
        "external_component" | "external_component_db" | "external_component_queue" => "#CCCCCC",
        _ => "#1168BD",
    }
}

fn default_border_color(kind: &str) -> &'static str {
    match kind {
        "person" => "#073B6F",
        "external_person" => "#8A8A8A",
        "system" | "system_db" | "system_queue" => "#3C7FC0",
        "external_system" | "external_system_db" | "external_system_queue" => "#8A8A8A",
        "container" | "container_db" | "container_queue" => "#3C7FC0",
        "external_container" | "external_container_db" | "external_container_queue" => "#A6A6A6",
        "component" | "component_db" | "component_queue" => "#78A8D8",
        "external_component" | "external_component_db" | "external_component_queue" => "#BFBFBF",
        _ => "#3C7FC0",
    }
}

// ── Font config helpers ─────────────────────────────────────────────────

impl C4Conf {
    fn shape_font_family(&self, _kind: &str) -> String {
        // Schema: each shape has its own *FontFamily defaulting to Open Sans.
        self.default_font_family.clone()
    }
    fn shape_font_size(&self, _kind: &str) -> f64 {
        self.default_font_size
    }
    fn boundaryFontFamily(&self) -> String {
        self.boundary_font_family.clone()
    }
}

// Suppress unused warnings for fields that exist but aren't read yet.
#[allow(dead_code)]
fn _suppress(b: &C4Boundary, r: &C4Rel, s: &C4Shape) {
    let _ = b;
    let _ = r;
    let _ = s;
}
