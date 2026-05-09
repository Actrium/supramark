//! Port of Go `d2layouts/d2grid` — grid layout engine for D2 diagrams.
//!
//! Handles objects with `grid-rows` / `grid-columns` properties, arranging
//! children in a 2D grid.

use crate::geo::{Point, Spacing};
use crate::graph::{Graph, ObjId};
use crate::label::Position;
use crate::shape::ShapeOps;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CONTAINER_PADDING: i32 = 60;
const DEFAULT_GAP: i32 = 40;

// Layout search constants
const STARTING_THRESHOLD: f64 = 1.2;
const THRESHOLD_STEP_SIZE: f64 = 0.25;
const MIN_THRESHOLD_ATTEMPTS: usize = 1;
const MAX_THRESHOLD_ATTEMPTS: usize = 3;
const ATTEMPT_LIMIT: usize = 100_000;
const SKIP_LIMIT: usize = 10_000_000;

// ---------------------------------------------------------------------------
// GridDiagram
// ---------------------------------------------------------------------------

struct GridDiagram {
    objects: Vec<ObjId>,
    edges: Vec<usize>, // edge indices into Graph.edges
    rows: usize,
    columns: usize,
    row_directed: bool,
    width: f64,
    height: f64,
    vertical_gap: i32,
    horizontal_gap: i32,
}

fn new_grid_diagram(g: &mut Graph, root: ObjId) -> GridDiagram {
    let obj = &g.objects[root];
    let children: Vec<ObjId> = obj.children_array.clone();

    let mut rows: usize = obj
        .grid_rows
        .as_ref()
        .and_then(|v| v.value.parse().ok())
        .unwrap_or(0);
    let mut columns: usize = obj
        .grid_columns
        .as_ref()
        .and_then(|v| v.value.parse().ok())
        .unwrap_or(0);

    let mut row_directed = false;

    if rows != 0 && columns != 0 {
        // Mirror Go d2grid: if the `grid-rows` key appears before
        // `grid-columns` in source order, the layout is row-directed.
        // Otherwise it is column-directed.
        if let (Some(rr), Some(cr)) = (
            obj.grid_rows_range.as_ref(),
            obj.grid_columns_range.as_ref(),
        ) {
            row_directed = rr.start.before(&cr.start);
        } else {
            // Fallback: default to row_directed when source ranges are
            // not available (e.g. programmatic construction in tests).
            row_directed = true;
        }

        // Expand grid to fit all objects
        let mut capacity = rows * columns;
        while capacity < children.len() {
            if row_directed {
                rows += 1;
                capacity += columns;
            } else {
                columns += 1;
                capacity += rows;
            }
        }
    } else if columns == 0 {
        row_directed = true;
        if children.len() < rows {
            rows = children.len();
        }
    } else if children.len() < columns {
        columns = children.len();
    }

    // Parse gap settings
    let mut vertical_gap = DEFAULT_GAP;
    let mut horizontal_gap = DEFAULT_GAP;
    if let Some(ref gap) = obj.grid_gap
        && let Ok(v) = gap.value.parse::<i32>()
    {
        vertical_gap = v;
        horizontal_gap = v;
    }
    if let Some(ref gap) = obj.vertical_gap
        && let Ok(v) = gap.value.parse::<i32>()
    {
        vertical_gap = v;
    }
    if let Some(ref gap) = obj.horizontal_gap
        && let Ok(v) = gap.value.parse::<i32>()
    {
        horizontal_gap = v;
    }

    // Reset all children positions
    for &child_id in &children {
        g.objects[child_id].top_left = Point { x: 0.0, y: 0.0 };
    }

    GridDiagram {
        objects: children,
        edges: Vec::new(),
        rows,
        columns,
        row_directed,
        width: 0.0,
        height: 0.0,
        vertical_gap,
        horizontal_gap,
    }
}

impl GridDiagram {
    fn shift(&self, g: &mut Graph, dx: f64, dy: f64) {
        for &obj_id in &self.objects {
            crate::graph::move_obj_with_descendants(g, obj_id, dx, dy);
        }
        for &ei in &self.edges {
            g.edges[ei].move_route(dx, dy);
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run grid layout on a graph whose root has grid-rows/grid-columns.
/// Mirrors Go `d2grid.Layout`.
pub fn layout(g: &mut Graph) -> Result<(), String> {
    let root = g.root;
    let mut gd = layout_grid(g, root)?;

    // Set default label/icon positions on root
    if g.objects[root].has_label() && g.objects[root].label_position.is_none() {
        g.objects[root].label_position = Some("INSIDE_TOP_CENTER".to_owned());
    }
    if g.objects[root].has_icon() && g.objects[root].icon_position.is_none() {
        g.objects[root].icon_position = Some("INSIDE_TOP_LEFT".to_owned());
    }

    // Resize container to fit grid content
    {
        let obj = &g.objects[root];
        let mut h_pad = CONTAINER_PADDING;
        let mut v_pad = CONTAINER_PADDING;
        if obj.grid_gap.is_some() || obj.horizontal_gap.is_some() {
            h_pad = gd.horizontal_gap;
        }
        if obj.grid_gap.is_some() || obj.vertical_gap.is_some() {
            v_pad = gd.vertical_gap;
        }

        let content_w = gd.width;
        let content_h = gd.height;

        let label_pos = obj
            .label_position
            .as_deref()
            .map(Position::from_string)
            .unwrap_or(Position::InsideMiddleCenter);
        let icon_pos = obj
            .icon_position
            .as_deref()
            .map(Position::from_string)
            .unwrap_or(Position::InsideMiddleCenter);

        let (_, mut padding) = obj.spacing();

        let label_w = if obj.label_dimensions.width > 0 {
            obj.label_dimensions.width as f64 + 2.0 * crate::label::PADDING
        } else {
            0.0
        };
        let label_h = if obj.label_dimensions.height > 0 {
            obj.label_dimensions.height as f64 + 2.0 * crate::label::PADDING
        } else {
            0.0
        };

        // Handle label overflow
        if label_w > 0.0 {
            use Position::*;
            match label_pos {
                OutsideTopLeft | OutsideTopCenter | OutsideTopRight | InsideTopLeft
                | InsideTopCenter | InsideTopRight | InsideBottomLeft | InsideBottomCenter
                | InsideBottomRight | OutsideBottomLeft | OutsideBottomCenter
                | OutsideBottomRight => {
                    let overflow = label_w - content_w;
                    if overflow > 0.0 {
                        padding.left += overflow / 2.0;
                        padding.right += overflow / 2.0;
                    }
                }
                _ => {}
            }
        }
        if label_h > 0.0 {
            use Position::*;
            match label_pos {
                OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom | InsideMiddleLeft
                | InsideMiddleCenter | InsideMiddleRight | OutsideRightTop | OutsideRightMiddle
                | OutsideRightBottom => {
                    let overflow = label_h - content_h;
                    if overflow > 0.0 {
                        padding.top += overflow / 2.0;
                        padding.bottom += overflow / 2.0;
                    }
                }
                _ => {}
            }
        }

        // Default label+icon spacing
        if icon_pos == Position::InsideTopLeft && label_pos == Position::InsideTopCenter {
            let icon_size = crate::target::MAX_ICON_SIZE as f64 + 2.0 * crate::label::PADDING;
            padding.left = padding.left.max(icon_size);
            padding.right = padding.right.max(icon_size);
            let min_w =
                2.0 * icon_size + obj.label_dimensions.width as f64 + 2.0 * crate::label::PADDING;
            let overflow = min_w - content_w;
            if overflow > 0.0 {
                padding.left = padding.left.max(overflow / 2.0);
                padding.right = padding.right.max(overflow / 2.0);
            }
        }

        padding.top = padding.top.max(v_pad as f64);
        padding.bottom = padding.bottom.max(v_pad as f64);
        padding.left = padding.left.max(h_pad as f64);
        padding.right = padding.right.max(h_pad as f64);

        let total_w = padding.left + content_w + padding.right;
        let total_h = padding.top + content_h + padding.bottom;

        g.objects[root].size_to_content(total_w, total_h, 0.0, 0.0);

        // Compute where the grid should be placed inside the shape.
        // Mirrors Go: s.GetInsidePlacement + innerBox centering.
        let dsl_shape = g.objects[root].shape.value.to_lowercase();
        let shape_type = crate::target::dsl_shape_to_shape_type(&dsl_shape);
        let bbox = crate::geo::Box2D::new(
            g.objects[root].top_left,
            g.objects[root].width,
            g.objects[root].height,
        );
        let mut s = crate::shape::Shape::new(shape_type, bbox);
        // Mirror Go's `obj.ToShape()` — propagate ContentAspectRatio so the
        // cloud's inner box uses the actor's aspect ratio (otherwise children
        // are placed using the bbox aspect, shifting them by tens of pixels).
        if shape_type == crate::shape::CLOUD_TYPE
            && let Some(ar) = g.objects[root].content_aspect_ratio
        {
            s.set_inner_box_aspect_ratio(ar);
        }
        let inner_tl = s.get_inside_placement(total_w, total_h, 0.0, 0.0);
        let inner_box = s.get_inner_box();
        let resize_dx = if inner_box.width > total_w {
            (inner_box.width - total_w) / 2.0
        } else {
            0.0
        };
        let resize_dy = if inner_box.height > total_h {
            (inner_box.height - total_h) / 2.0
        } else {
            0.0
        };

        let dx = -(h_pad as f64) + inner_tl.x + padding.left + resize_dx;
        let dy = -(v_pad as f64) + inner_tl.y + padding.top + resize_dy;
        if dx != 0.0 || dy != 0.0 {
            gd.shift(g, dx, dy);
        }
    }

    // Simple edge routing between grid children. Mirrors Go `d2grid.Layout`
    // which registers every edge whose endpoint parent is a descendant of the
    // grid root in `gd.edges`, so the final `gd.shift` (below) — applied when
    // this grid is nested (root_level > 0) and moves the whole grid off (0,0)
    // by `(top_left + h_pad, top_left + v_pad)` — carries the edge routes
    // along with the cells. Without this, routes for edges whose src/dst sit
    // inside a grid cell (e.g. a 3-row inner-inner grid's `1 -> 2 -> 3`) keep
    // their pre-shift coordinates and end up `(-h_pad, -v_pad)` off their
    // shape endpoints after the outer layout translates the grid container.
    let edge_indices: Vec<usize> = (0..g.edges.len()).collect();
    let mut grid_edge_indices = Vec::new();
    for ei in edge_indices {
        let src_parent = g.objects[g.edges[ei].src].parent;
        let dst_parent = g.objects[g.edges[ei].dst].parent;

        let src_in_grid = src_parent == Some(root) || {
            let sp = g.objects[g.edges[ei].src].parent;
            sp.is_some_and(|p| is_descendant_of(g, p, root))
        };
        let dst_in_grid = dst_parent == Some(root) || {
            let dp = g.objects[g.edges[ei].dst].parent;
            dp.is_some_and(|p| is_descendant_of(g, p, root))
        };

        if !src_in_grid && !dst_in_grid {
            continue;
        }
        grid_edge_indices.push(ei);
        gd.edges.push(ei);

        // Only do simple routing for direct children of the grid
        if src_parent != Some(root) || dst_parent != Some(root) {
            continue;
        }

        let src_id = g.edges[ei].src;
        let dst_id = g.edges[ei].dst;
        let src_center = g.objects[src_id].center();
        let dst_center = g.objects[dst_id].center();
        let mut points = vec![src_center, dst_center];

        // Trim endpoints to shape boundaries. Mirrors Go
        // `Edge.TraceToShape(points, 0, 1)` which mutates `points` in place.
        // For grid children the boxes are rectangles; we intersect the
        // center-to-center segment with each box and snap to the border.
        let src_box = crate::geo::Box2D::new(
            g.objects[src_id].top_left,
            g.objects[src_id].width,
            g.objects[src_id].height,
        );
        let dst_box = crate::geo::Box2D::new(
            g.objects[dst_id].top_left,
            g.objects[dst_id].width,
            g.objects[dst_id].height,
        );
        let starting_segment = crate::geo::Segment::new(points[1], points[0]);
        if let Some(p) = src_box.intersections(&starting_segment).first().copied() {
            points[0] = p;
        }
        let ending_segment = crate::geo::Segment::new(points[0], points[1]);
        if let Some(p) = dst_box.intersections(&ending_segment).first().copied() {
            points[1] = p;
        }
        g.edges[ei].route = points;

        if !g.edges[ei].label.value.is_empty() {
            g.edges[ei].label_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
        }
    }

    // Set root position
    if g.objects[root].is_grid_diagram() && !g.objects[root].children_array.is_empty() {
        g.objects[root].top_left = Point { x: 0.0, y: 0.0 };
    }

    // Shift for nested grids
    if g.root_level > 0 {
        let obj = &g.objects[root];
        let mut h_pad = CONTAINER_PADDING;
        let mut v_pad = CONTAINER_PADDING;
        if obj.grid_gap.is_some() || obj.horizontal_gap.is_some() {
            h_pad = gd.horizontal_gap;
        }
        if obj.grid_gap.is_some() || obj.vertical_gap.is_some() {
            v_pad = gd.vertical_gap;
        }
        let sx = obj.top_left.x + h_pad as f64;
        let sy = obj.top_left.y + v_pad as f64;
        gd.shift(g, sx, sy);
    }

    Ok(())
}

fn is_descendant_of(g: &Graph, obj_id: ObjId, ancestor_id: ObjId) -> bool {
    let mut cur = g.objects[obj_id].parent;
    while let Some(pid) = cur {
        if pid == ancestor_id {
            return true;
        }
        cur = g.objects[pid].parent;
    }
    false
}

// ---------------------------------------------------------------------------
// Core layout logic
// ---------------------------------------------------------------------------

fn layout_grid(g: &mut Graph, root: ObjId) -> Result<GridDiagram, String> {
    let mut gd = new_grid_diagram(g, root);

    // Position labels and icons on children
    for &obj_id in &gd.objects {
        let obj = &g.objects[obj_id];
        let has_icon = obj.has_icon();
        let has_label = obj.has_label();
        let is_container = !obj.children_array.is_empty();
        let has_outside_bottom = obj.has_outside_bottom_label();
        let icon_pos_set = obj.icon_position.is_some();
        let label_pos_set = obj.label_position.is_some();

        let mut positioned_label = false;
        if has_icon && !icon_pos_set {
            if is_container {
                g.objects[obj_id].icon_position = Some("OUTSIDE_TOP_LEFT".to_owned());
                if !label_pos_set {
                    g.objects[obj_id].label_position = Some("OUTSIDE_TOP_RIGHT".to_owned());
                    positioned_label = true;
                }
            } else {
                g.objects[obj_id].icon_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
            }
        }
        if !positioned_label && has_label && !label_pos_set {
            if is_container {
                g.objects[obj_id].label_position = Some("OUTSIDE_TOP_CENTER".to_owned());
            } else if has_outside_bottom {
                g.objects[obj_id].label_position = Some("OUTSIDE_BOTTOM_CENTER".to_owned());
            } else if has_icon {
                g.objects[obj_id].label_position = Some("INSIDE_TOP_CENTER".to_owned());
            } else {
                g.objects[obj_id].label_position = Some("INSIDE_MIDDLE_CENTER".to_owned());
            }
        }
    }

    // Adjust sizes for outside labels
    let margins = size_for_outside_labels(g, &gd.objects);

    if gd.rows != 0 && gd.columns != 0 {
        layout_evenly(g, &mut gd);
    } else {
        layout_dynamic(g, &mut gd);
    }

    // Revert outside label adjustments
    revert_outside_labels(g, &margins);

    Ok(gd)
}

// ---------------------------------------------------------------------------
// Even layout (both rows and columns specified)
// ---------------------------------------------------------------------------

fn layout_evenly(g: &mut Graph, gd: &mut GridDiagram) {
    let get_index = |row: usize, col: usize| -> Option<usize> {
        let idx = if gd.row_directed {
            row * gd.columns + col
        } else {
            col * gd.rows + row
        };
        if idx < gd.objects.len() {
            Some(idx)
        } else {
            None
        }
    };

    // Compute row heights and column widths
    let mut row_heights = vec![0.0f64; gd.rows];
    let mut col_widths = vec![0.0f64; gd.columns];

    for (i, row) in row_heights.iter_mut().enumerate() {
        for (j, col) in col_widths.iter_mut().enumerate() {
            if let Some(idx) = get_index(i, j) {
                let obj = &g.objects[gd.objects[idx]];
                *row = row.max(obj.height);
                *col = col.max(obj.width);
            }
        }
    }

    let h_gap = gd.horizontal_gap as f64;
    let v_gap = gd.vertical_gap as f64;

    if gd.row_directed {
        let mut y = 0.0;
        for (i, &row_h) in row_heights.iter().enumerate() {
            let mut x = 0.0;
            for (j, &col_w) in col_widths.iter().enumerate() {
                if let Some(idx) = get_index(i, j) {
                    let oid = gd.objects[idx];
                    g.objects[oid].width = col_w;
                    g.objects[oid].height = row_h;
                    crate::graph::move_obj_with_descendants_to(g, oid, x, y);
                    x += col_w + h_gap;
                }
            }
            y += row_h + v_gap;
        }
    } else {
        let mut x = 0.0;
        for (j, &col_w) in col_widths.iter().enumerate() {
            let mut y = 0.0;
            for (i, &row_h) in row_heights.iter().enumerate() {
                if let Some(idx) = get_index(i, j) {
                    let oid = gd.objects[idx];
                    g.objects[oid].width = col_w;
                    g.objects[oid].height = row_h;
                    crate::graph::move_obj_with_descendants_to(g, oid, x, y);
                    y += row_h + v_gap;
                }
            }
            x += col_w + h_gap;
        }
    }

    let total_w: f64 = col_widths.iter().sum::<f64>() + h_gap * (gd.columns as f64 - 1.0).max(0.0);
    let total_h: f64 = row_heights.iter().sum::<f64>() + v_gap * (gd.rows as f64 - 1.0).max(0.0);
    gd.width = total_w;
    gd.height = total_h;
}

// ---------------------------------------------------------------------------
// Dynamic layout (only rows OR columns specified)
// ---------------------------------------------------------------------------

fn layout_dynamic(g: &mut Graph, gd: &mut GridDiagram) {
    let h_gap = gd.horizontal_gap as f64;
    let v_gap = gd.vertical_gap as f64;

    // Compute total dimensions and target size
    let mut total_w = 0.0f64;
    let mut total_h = 0.0f64;
    for &oid in &gd.objects {
        total_w += g.objects[oid].width;
        total_h += g.objects[oid].height;
    }
    total_w += h_gap * (gd.objects.len() as f64 - gd.rows as f64);
    total_h += v_gap * (gd.objects.len() as f64 - gd.columns as f64);

    let layout = if gd.row_directed {
        let target = total_w / gd.rows as f64;
        get_best_layout(g, gd, target, false)
    } else {
        let target = total_h / gd.columns as f64;
        get_best_layout(g, gd, target, true)
    };

    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;

    if gd.row_directed {
        // Measure row widths
        let mut row_widths = Vec::new();
        for row in &layout {
            let rw: f64 = row.iter().map(|&oid| g.objects[oid].width).sum::<f64>()
                + h_gap * (row.len() as f64 - 1.0).max(0.0);
            row_widths.push(rw);
            max_x = max_x.max(rw);
        }

        // Expand thinnest objects to make each row the same width
        for (i, row) in layout.iter().enumerate() {
            let rw = row_widths[i];
            if rw >= max_x {
                continue;
            }
            let delta = max_x - rw;
            let widest: f64 = row
                .iter()
                .map(|&oid| g.objects[oid].width)
                .fold(0.0, f64::max);
            let mut diffs: Vec<f64> = row
                .iter()
                .map(|&oid| widest - g.objects[oid].width)
                .collect();
            let total_diff: f64 = diffs.iter().sum();
            if total_diff > 0.0 {
                for d in &mut diffs {
                    *d /= total_diff;
                }
                let growth = delta.min(total_diff);
                for (j, &oid) in row.iter().enumerate() {
                    g.objects[oid].width += diffs[j] * growth;
                }
            }
            if delta > total_diff {
                let growth = (delta - total_diff) / row.len() as f64;
                for &oid in row {
                    g.objects[oid].width += growth;
                }
            }
        }

        // Position objects
        let mut cy = 0.0;
        for row in &layout {
            let mut cx = 0.0;
            let mut row_height = 0.0f64;
            for &oid in row {
                crate::graph::move_obj_with_descendants_to(g, oid, cx, cy);
                cx += g.objects[oid].width + h_gap;
                row_height = row_height.max(g.objects[oid].height);
            }
            for &oid in row {
                g.objects[oid].height = row_height;
            }
            cy += row_height + v_gap;
        }
        max_y = cy - v_gap;
    } else {
        // Measure column heights
        let mut col_heights = Vec::new();
        for col in &layout {
            let ch: f64 = col.iter().map(|&oid| g.objects[oid].height).sum::<f64>()
                + v_gap * (col.len() as f64 - 1.0).max(0.0);
            col_heights.push(ch);
            max_y = max_y.max(ch);
        }

        // Expand shortest objects to make each column the same height
        for (i, col) in layout.iter().enumerate() {
            let ch = col_heights[i];
            if ch >= max_y {
                continue;
            }
            let delta = max_y - ch;
            let tallest: f64 = col
                .iter()
                .map(|&oid| g.objects[oid].height)
                .fold(0.0, f64::max);
            let mut diffs: Vec<f64> = col
                .iter()
                .map(|&oid| tallest - g.objects[oid].height)
                .collect();
            let total_diff: f64 = diffs.iter().sum();
            if total_diff > 0.0 {
                for d in &mut diffs {
                    *d /= total_diff;
                }
                let growth = delta.min(total_diff);
                for (j, &oid) in col.iter().enumerate() {
                    g.objects[oid].height += diffs[j] * growth;
                }
            }
            if delta > total_diff {
                let growth = (delta - total_diff) / col.len() as f64;
                for &oid in col {
                    g.objects[oid].height += growth;
                }
            }
        }

        // Position objects
        let mut cx = 0.0;
        for col in &layout {
            let mut cy = 0.0;
            let mut col_width = 0.0f64;
            for &oid in col {
                crate::graph::move_obj_with_descendants_to(g, oid, cx, cy);
                cy += g.objects[oid].height + v_gap;
                col_width = col_width.max(g.objects[oid].width);
            }
            for &oid in col {
                g.objects[oid].width = col_width;
            }
            cx += col_width + h_gap;
        }
        max_x = cx - h_gap;
    }

    gd.width = max_x;
    gd.height = max_y;
}

// ---------------------------------------------------------------------------
// Best layout search
// ---------------------------------------------------------------------------

/// State shared between the division search callbacks.
struct SearchState {
    best_layout: Option<Vec<Vec<ObjId>>>,
    best_dist: f64,
    fast_is_best: bool,
    count: usize,
    skip_count: usize,
    starting_cache: std::collections::HashMap<usize, bool>,
}

fn get_best_layout(
    g: &Graph,
    gd: &GridDiagram,
    target_size: f64,
    columns: bool,
) -> Vec<Vec<ObjId>> {
    let n_cuts = if columns {
        gd.columns.saturating_sub(1)
    } else {
        gd.rows.saturating_sub(1)
    };
    if n_cuts == 0 {
        return vec![gd.objects.clone()];
    }

    let mut state = SearchState {
        best_layout: None,
        best_dist: f64::MAX,
        fast_is_best: false,
        count: 0,
        skip_count: 0,
        starting_cache: std::collections::HashMap::new(),
    };

    // Try fast layout first
    if let Some(fl) = fast_layout(g, gd, target_size, n_cuts, columns) {
        let dist = get_dist_to_target(
            g,
            &fl,
            target_size,
            gd.horizontal_gap as f64,
            gd.vertical_gap as f64,
            columns,
        );
        if dist == 0.0 {
            return fl;
        }
        state.best_dist = dist;
        state.best_layout = Some(fl);
        state.fast_is_best = true;
    }

    let gap = if columns {
        gd.vertical_gap as f64
    } else {
        gd.horizontal_gap as f64
    };

    let sizes: Vec<f64> = gd
        .objects
        .iter()
        .map(|&oid| {
            if columns {
                g.objects[oid].height
            } else {
                g.objects[oid].width
            }
        })
        .collect();
    let sd = stddev(&sizes);

    let mut threshold_attempts =
        (sd.ceil() as usize).clamp(MIN_THRESHOLD_ATTEMPTS, MAX_THRESHOLD_ATTEMPTS);
    let mut ok_threshold = STARTING_THRESHOLD;

    let mut attempt = 0;
    while attempt < threshold_attempts || state.best_layout.is_none() {
        state.count = 0;
        state.skip_count = 0;

        let params = DivisionSearchParams {
            g,
            gd,
            target_size,
            columns,
            gap,
            ok_threshold,
        };
        iter_divisions_search(&sizes, n_cuts, &mut Vec::new(), &params, &mut state);

        ok_threshold += THRESHOLD_STEP_SIZE;
        state.starting_cache.clear();

        if state.skip_count == 0 {
            break;
        }
        if state.count == 0 && threshold_attempts < MAX_THRESHOLD_ATTEMPTS {
            threshold_attempts += 1;
        }
        attempt += 1;
    }

    state
        .best_layout
        .unwrap_or_else(|| vec![gd.objects.clone()])
}

/// Immutable search context threaded through `iter_divisions_search` recursion.
struct DivisionSearchParams<'a> {
    g: &'a Graph,
    gd: &'a GridDiagram,
    target_size: f64,
    columns: bool,
    gap: f64,
    ok_threshold: f64,
}

/// Port of Go `iterDivisions`, carrying pending outer cuts in reverse order.
fn iter_divisions_search(
    sizes: &[f64],
    n_cuts: usize,
    pending_suffix: &mut Vec<usize>,
    params: &DivisionSearchParams<'_>,
    state: &mut SearchState,
) -> bool {
    if sizes.len() < 2 || n_cuts == 0 {
        return false;
    }
    let last_obj = sizes.len() - 1;
    for index in (n_cuts..=last_obj).rev() {
        if !check_cut(
            &sizes[index..],
            false,
            params.gap,
            params.ok_threshold,
            params.target_size,
            state,
        ) {
            continue;
        }
        if n_cuts > 1 {
            pending_suffix.push(index - 1);
            let done =
                iter_divisions_search(&sizes[..index], n_cuts - 1, pending_suffix, params, state);
            pending_suffix.pop();
            if done {
                return true;
            }
        } else {
            if !check_cut(
                &sizes[..index],
                true,
                params.gap,
                params.ok_threshold,
                params.target_size,
                state,
            ) {
                continue;
            }
            let mut division = Vec::with_capacity(pending_suffix.len() + 1);
            division.push(index - 1);
            division.extend(pending_suffix.iter().rev().copied());
            let layout = gen_layout(&params.gd.objects, &division);
            let dist = get_dist_to_target(
                params.g,
                &layout,
                params.target_size,
                params.gd.horizontal_gap as f64,
                params.gd.vertical_gap as f64,
                params.columns,
            );
            if dist < state.best_dist || (state.fast_is_best && dist == state.best_dist) {
                state.best_layout = Some(layout);
                state.best_dist = dist;
                state.fast_is_best = false;
            }
            state.count += 1;
            if state.count >= ATTEMPT_LIMIT || state.skip_count >= SKIP_LIMIT {
                return true;
            }
        }
    }
    false
}

fn check_cut(
    row_sizes: &[f64],
    starting: bool,
    gap: f64,
    ok_threshold: f64,
    target_size: f64,
    state: &mut SearchState,
) -> bool {
    if starting && let Some(&cached) = state.starting_cache.get(&row_sizes.len()) {
        return cached;
    }
    let mut row_size: f64 = row_sizes.iter().sum();
    if row_sizes.len() > 1 {
        row_size += gap * (row_sizes.len() as f64 - 1.0);
        if row_size > ok_threshold * target_size {
            state.skip_count += 1;
            let ok = state.skip_count >= SKIP_LIMIT;
            if starting {
                state.starting_cache.insert(row_sizes.len(), ok);
            }
            return ok;
        }
    }
    if row_size < target_size / ok_threshold {
        state.skip_count += 1;
        let ok = state.skip_count >= SKIP_LIMIT;
        if starting {
            state.starting_cache.insert(row_sizes.len(), ok);
        }
        return ok;
    }
    if starting {
        state.starting_cache.insert(row_sizes.len(), true);
    }
    true
}

fn fast_layout(
    g: &Graph,
    gd: &GridDiagram,
    target_size: f64,
    n_cuts: usize,
    columns: bool,
) -> Option<Vec<Vec<ObjId>>> {
    let gap = if columns {
        gd.vertical_gap as f64
    } else {
        gd.horizontal_gap as f64
    };

    let mut debt = 0.0;
    let mut fast_division = Vec::with_capacity(n_cuts);
    let mut row_size = 0.0;

    for (i, &oid) in gd.objects.iter().enumerate() {
        let size = if columns {
            g.objects[oid].height
        } else {
            g.objects[oid].width
        };

        if row_size == 0.0 {
            if size > target_size - debt {
                fast_division.push(i);
                debt += size - target_size;
            } else {
                row_size += size;
            }
            continue;
        }
        if row_size + gap + size / 2.0 > target_size - debt {
            fast_division.push(i - 1);
            debt += row_size - target_size;
            row_size = size;
        } else {
            row_size += gap + size;
        }
    }

    if fast_division.len() == n_cuts {
        Some(gen_layout(&gd.objects, &fast_division))
    } else {
        None
    }
}

fn gen_layout(objects: &[ObjId], cut_indices: &[usize]) -> Vec<Vec<ObjId>> {
    let mut layout = Vec::with_capacity(cut_indices.len() + 1);
    let mut obj_index = 0;
    for i in 0..=cut_indices.len() {
        let stop = if i < cut_indices.len() {
            cut_indices[i]
        } else {
            objects.len() - 1
        };
        let mut row = Vec::new();
        while obj_index <= stop {
            row.push(objects[obj_index]);
            obj_index += 1;
        }
        layout.push(row);
    }
    layout
}

fn get_dist_to_target(
    g: &Graph,
    layout: &[Vec<ObjId>],
    target_size: f64,
    h_gap: f64,
    v_gap: f64,
    columns: bool,
) -> f64 {
    let mut total_delta = 0.0;
    for row in layout {
        let mut row_size = 0.0;
        for &oid in row {
            if columns {
                row_size += g.objects[oid].height + v_gap;
            } else {
                row_size += g.objects[oid].width + h_gap;
            }
        }
        if !row.is_empty() {
            if columns {
                row_size -= v_gap;
            } else {
                row_size -= h_gap;
            }
        }
        total_delta += (row_size - target_size).abs();
    }
    total_delta
}

// ---------------------------------------------------------------------------
// Outside label sizing
// ---------------------------------------------------------------------------

fn size_for_outside_labels(g: &mut Graph, objects: &[ObjId]) -> Vec<(ObjId, Spacing)> {
    let mut margins = Vec::new();
    for &oid in objects {
        let margin = g.objects[oid].get_margin();
        margins.push((oid, margin));
        g.objects[oid].height += margin.top + margin.bottom;
        g.objects[oid].width += margin.left + margin.right;
    }
    margins
}

fn revert_outside_labels(g: &mut Graph, margins: &[(ObjId, Spacing)]) {
    for &(oid, m) in margins {
        let dy = m.top + m.bottom;
        let dx = m.left + m.right;
        g.objects[oid].height -= dy;
        g.objects[oid].width -= dx;

        // Less margin may be needed if layout grew the object
        let new_margin = g.objects[oid].get_margin();
        let margin_x = new_margin.left + new_margin.right;
        let margin_y = new_margin.top + new_margin.bottom;
        if margin_x < dx {
            g.objects[oid].width += dx - margin_x;
        }
        if margin_y < dy {
            g.objects[oid].height += dy - margin_y;
        }

        if new_margin.left > 0.0 || new_margin.top > 0.0 {
            crate::graph::move_obj_with_descendants(g, oid, new_margin.left, new_margin.top);
        }
    }
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

fn stddev(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
    let variance: f64 =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gen_layout_no_cuts() {
        let objects = vec![0, 1, 2, 3, 4];
        let result = gen_layout(&objects, &[]);
        assert_eq!(result, vec![vec![0, 1, 2, 3, 4]]);
    }

    #[test]
    fn gen_layout_single_cut() {
        let objects = vec![0, 1, 2, 3, 4];
        let result = gen_layout(&objects, &[2]);
        assert_eq!(result, vec![vec![0, 1, 2], vec![3, 4]]);
    }

    #[test]
    fn gen_layout_two_cuts() {
        let objects = vec![0, 1, 2, 3, 4];
        let result = gen_layout(&objects, &[0, 2]);
        assert_eq!(result, vec![vec![0], vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn gen_layout_three_cuts() {
        let objects: Vec<usize> = (0..8).collect();
        let result = gen_layout(&objects, &[0, 2, 6]);
        assert_eq!(result, vec![vec![0], vec![1, 2], vec![3, 4, 5, 6], vec![7]]);
    }

    #[test]
    fn stddev_uniform() {
        let vals = vec![5.0, 5.0, 5.0, 5.0];
        assert!((stddev(&vals) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn stddev_varied() {
        let vals = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        // population stddev = 2.0
        assert!((stddev(&vals) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_new_grid_diagram_rows_only() {
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_rows = Some(crate::graph::ScalarValue {
            value: "3".to_owned(),
        });

        // Add 5 children
        for i in 0..5 {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("child{}", i),
                abs_id: format!("child{}", i),
                parent: Some(root),
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        let gd = new_grid_diagram(&mut g, root);
        assert_eq!(gd.rows, 3);
        assert_eq!(gd.columns, 0);
        assert!(gd.row_directed);
    }

    #[test]
    fn test_new_grid_diagram_cols_only() {
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_columns = Some(crate::graph::ScalarValue {
            value: "2".to_owned(),
        });

        for i in 0..4 {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("c{}", i),
                abs_id: format!("c{}", i),
                parent: Some(root),
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        let gd = new_grid_diagram(&mut g, root);
        assert_eq!(gd.rows, 0);
        assert_eq!(gd.columns, 2);
        assert!(!gd.row_directed);
    }

    #[test]
    fn test_layout_evenly_2x2() {
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_rows = Some(crate::graph::ScalarValue {
            value: "2".to_owned(),
        });
        g.objects[root].grid_columns = Some(crate::graph::ScalarValue {
            value: "2".to_owned(),
        });

        // Add 4 children with different sizes
        let sizes = [(50.0, 30.0), (80.0, 40.0), (60.0, 50.0), (70.0, 35.0)];
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("n{}", i),
                abs_id: format!("n{}", i),
                parent: Some(root),
                width: w,
                height: h,
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        let mut gd = new_grid_diagram(&mut g, root);
        layout_evenly(&mut g, &mut gd);

        // Objects: 1=n0(50,30), 2=n1(80,40), 3=n2(60,50), 4=n3(70,35)
        // row_directed=true, getObject(r,c) = r*cols+c, objects=[1,2,3,4]
        // (0,0)=n0, (0,1)=n1, (1,0)=n2, (1,1)=n3
        // col widths: col0=max(50,60)=60, col1=max(80,70)=80
        // row heights: row0=max(30,40)=40, row1=max(50,35)=50
        assert_eq!(g.objects[1].width, 60.0, "n0 col0 width");
        assert_eq!(g.objects[2].width, 80.0, "n1 col1 width");
        assert_eq!(g.objects[1].height, 40.0, "n0 row0 height");
        assert_eq!(g.objects[3].height, 50.0, "n2 row1 height");

        // Total: width = 60 + gap(40) + 80 = 180; height = 40 + gap(40) + 50 = 130
        assert_eq!(gd.width, 60.0 + 40.0 + 80.0);
        assert_eq!(gd.height, 40.0 + 40.0 + 50.0);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_3_rows_3_items() {
        // Mirrors grid_rows_gap_bug: 3 rows, 3 items, h-gap=100, v-gap=0
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_rows = Some(crate::graph::ScalarValue {
            value: "3".to_owned(),
        });
        g.objects[root].horizontal_gap = Some(crate::graph::ScalarValue {
            value: "100".to_owned(),
        });
        g.objects[root].vertical_gap = Some(crate::graph::ScalarValue {
            value: "0".to_owned(),
        });

        let sizes = [(53.0, 66.0), (66.0, 66.0), (53.0, 66.0)]; // typical measured sizes
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("item{}", i),
                abs_id: format!("item{}", i),
                parent: Some(root),
                width: w,
                height: h,
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        let gd = new_grid_diagram(&mut g, root);
        eprintln!(
            "rows={}, cols={}, row_directed={}",
            gd.rows, gd.columns, gd.row_directed
        );
        assert_eq!(gd.rows, 3);
        assert_eq!(gd.columns, 0);
        assert!(gd.row_directed);
    }

    #[test]
    fn test_3_rows_layout_dimensions() {
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_rows = Some(crate::graph::ScalarValue {
            value: "3".to_owned(),
        });
        g.objects[root].horizontal_gap = Some(crate::graph::ScalarValue {
            value: "100".to_owned(),
        });
        g.objects[root].vertical_gap = Some(crate::graph::ScalarValue {
            value: "0".to_owned(),
        });

        let sizes = [(53.0, 66.0), (66.0, 66.0), (53.0, 66.0)];
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("item{}", i),
                abs_id: format!("item{}", i),
                parent: Some(root),
                width: w,
                height: h,
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        layout(&mut g).expect("layout");

        eprintln!("root: {}x{}", g.objects[root].width, g.objects[root].height);
        for i in 1..=3 {
            eprintln!(
                "  item{}: ({}, {}) {}x{}",
                i - 1,
                g.objects[i].top_left.x,
                g.objects[i].top_left.y,
                g.objects[i].width,
                g.objects[i].height
            );
        }
        // 3 rows, each with 1 item, v-gap=0.
        // Items should be stacked vertically, each at y=i*66.
        assert_eq!(g.objects[1].top_left.y, 0.0, "item0 y");
        assert_eq!(g.objects[2].top_left.y, 66.0, "item1 y");
        assert_eq!(g.objects[3].top_left.y, 132.0, "item2 y");
        // All items should be in the same column (same x)
        assert_eq!(
            g.objects[1].top_left.x, g.objects[2].top_left.x,
            "items same x"
        );
        // Items should all be expanded to max width (66)
        assert_eq!(g.objects[1].width, 66.0, "item0 width");
        assert_eq!(g.objects[2].width, 66.0, "item1 width (was widest)");
        assert_eq!(g.objects[3].width, 66.0, "item2 width");
    }

    #[test]
    fn test_2x2_evenly_layout_full() {
        let mut g = Graph::new();
        let root = g.root;
        g.objects[root].grid_rows = Some(crate::graph::ScalarValue {
            value: "2".to_owned(),
        });
        g.objects[root].grid_columns = Some(crate::graph::ScalarValue {
            value: "2".to_owned(),
        });
        g.objects[root].grid_gap = Some(crate::graph::ScalarValue {
            value: "0".to_owned(),
        });

        let sizes = [(100.0, 100.0); 4];
        for (i, &(w, h)) in sizes.iter().enumerate() {
            let id = g.objects.len();
            g.objects.push(crate::graph::Object {
                id: format!("n{}", i),
                abs_id: format!("n{}", i),
                parent: Some(root),
                width: w,
                height: h,
                ..Default::default()
            });
            g.objects[root].children_array.push(id);
        }

        layout(&mut g).expect("layout");

        eprintln!("root: {}x{}", g.objects[root].width, g.objects[root].height);
        // 2x2 grid of 100x100 items with gap 0 => content 200x200
        // With padding the root should be bigger
        assert!(
            g.objects[root].width >= 200.0,
            "Root width should be >= 200: {}",
            g.objects[root].width
        );
        assert!(
            g.objects[root].height >= 200.0,
            "Root height should be >= 200: {}",
            g.objects[root].height
        );
    }
}
