//! gitGraph layout — port of upstream `gitGraphRenderer.ts` LR mode.
//!
//! Supports:
//!   - linear, single-branch fixtures (commits in chronological order)
//!   - multi-branch (`branch foo` + `checkout`) with one lane per branch
//!   - `merge` commits (two parents, double-circle bullet)
//!
//! Unsupported (still falls back to known-ignored):
//!   - TB/BT orientations
//!   - `cherry-pick`
//!   - `parallelCommits`
//!   - redux-geometry themes (REDUX_GEOMETRY_THEMES)
//!
//! Geometry constants (LR, non-redux):
//!   - lane spacing on Y axis: 50 + (rotateCommitLabel ? 40 : 0) → 90 by default
//!   - X step per commit: COMMIT_STEP + LAYOUT_OFFSET = 50
//!   - bullet centre y = lane y - 2 (the spine is offset from `branch.pos`)
//!   - merge bullet: outer r=10, inner r=6
//!
//! See `src/render/svg_gitgraph.rs` for the corresponding emitter.

use crate::error::Result;
use crate::font_metrics;
use crate::model::gitgraph::{CommitKind, GitGraphDiagram, Orientation};
use crate::theme::ThemeVariables;

pub const LAYOUT_OFFSET: f64 = 10.0;
pub const COMMIT_STEP: f64 = 40.0;
/// `defaultPos` from upstream — the initial cursor offset for TB/BT.
pub const DEFAULT_POS: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct BranchPosition {
    pub name: String,
    /// `branch.pos` from upstream — the reference y for the lane (the
    /// dotted spine line is drawn at `pos - 2`).
    pub pos: f64,
    pub index: usize,
    /// Width / height of the rendered branch-name label (used to size
    /// the rounded rect that sits to the left of the LR spine).
    pub label_width: f64,
    pub label_height: f64,
}

#[derive(Debug, Clone)]
pub struct CommitGeom {
    pub id: String,
    pub seq: usize,
    /// Center point of the bullet circle.
    pub cx: f64,
    pub cy: f64,
    /// `pos + LAYOUT_OFFSET` (used by upstream's label/tag math).
    pub pos_with_offset: f64,
    /// `pos` itself (the running cursor before adding LAYOUT_OFFSET).
    pub pos: f64,
    /// `0`-based branch lane index — used for color-class numbering.
    pub branch_index: usize,
}

#[derive(Debug, Clone)]
pub struct GitGraphLayout {
    pub orientation: Orientation,
    /// True for `gitGraph TB:` (top-to-bottom) — the renderer flips its
    /// per-element math when this is set.
    pub is_tb: bool,
    /// True for `gitGraph BT:` (bottom-to-top).
    pub is_bt: bool,
    /// Branches in **render** order (sorted by `order` then insertion).
    pub branches: Vec<BranchPosition>,
    pub commits: Vec<CommitGeom>,
    pub max_pos: f64,
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
    /// Height of the branch-label text used for vertical alignment math
    /// (rect y/transform). Same value across branches under the bbox shim.
    pub branch_label_height: f64,
    pub commit_label_height: f64,
    /// Actual measured widths of each commit-label, in the same order
    /// as `commits`. Used to position the per-commit `<rect>` background.
    pub commit_label_widths: Vec<f64>,
    /// Height of the commit-label text.
    pub commit_label_text_height: f64,
    /// Title text x-coordinate (centred over the pre-title bbox).
    pub title_x: f64,
}

const FONT_FAMILY: &str = "sans-serif";
const LABEL_SIZE: f64 = 14.0;

/// Sort branches by `order` (ascending) using `0.{insertion-index}` as
/// the implicit value when `order` is unset, mirroring upstream's
/// `getBranchesAsObjArray`.
fn sort_branches_by_order(d: &GitGraphDiagram) -> Vec<usize> {
    let mut indexed: Vec<(usize, f64)> = d
        .branches
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let key = match b.order {
                Some(v) => v as f64,
                None => format!("0.{i}").parse::<f64>().unwrap_or(i as f64),
            };
            (i, key)
        })
        .collect();
    indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    indexed.into_iter().map(|(i, _)| i).collect()
}

pub fn layout(d: &GitGraphDiagram, _theme: &ThemeVariables) -> Result<GitGraphLayout> {
    let is_tb = matches!(d.orientation, Orientation::TB);
    let is_bt = matches!(d.orientation, Orientation::BT);
    let is_vert = is_tb || is_bt;

    // Branch label widths — measured at 14px sans-serif (jsdom shim does
    // not honour CSS, so the bbox always reports the default-font width).
    let label_h = font_metrics::line_height(FONT_FAMILY, LABEL_SIZE, false, false);

    let order = sort_branches_by_order(d);
    let mut branch_positions: Vec<BranchPosition> = Vec::with_capacity(d.branches.len());
    let mut pos: f64 = 0.0;
    let rotate_term = if d.config.rotate_commit_label { 40.0 } else { 0.0 };
    for (idx, &orig_idx) in order.iter().enumerate() {
        let b = &d.branches[orig_idx];
        let lw = font_metrics::text_width(&b.name, FONT_FAMILY, LABEL_SIZE, false, false);
        branch_positions.push(BranchPosition {
            name: b.name.clone(),
            pos,
            index: idx,
            label_width: lw,
            label_height: label_h,
        });
        // setBranchPosition increment: 50 + (rotate ? 40 : 0) + (TB|BT ? bbox/2 : 0)
        let tb_term = if is_vert { lw / 2.0 } else { 0.0 };
        pos += 50.0 + rotate_term + tb_term;
    }

    // Commit positions along the running axis (X for LR, Y for TB/BT).
    // Positions match upstream's `drawCommits`.
    // For BT, upstream reverses the sorted-keys list so the iteration
    // goes from latest seq to oldest. We mirror that by iterating
    // d.commits in reverse for BT, then re-sort by seq before writing
    // back so `commits[i]` still aligns with `d.commits[i]`.
    let mut commits: Vec<CommitGeom> = Vec::with_capacity(d.commits.len());
    let mut cursor: f64 = if is_vert { DEFAULT_POS } else { 0.0 };
    let mut max_pos: f64 = if is_vert { DEFAULT_POS } else { 0.0 };
    let label_widths: Vec<f64> = d
        .commits
        .iter()
        .map(|c| font_metrics::text_width(&c.id, FONT_FAMILY, LABEL_SIZE, false, false))
        .collect();
    let commit_label_text_height =
        font_metrics::line_height(FONT_FAMILY, LABEL_SIZE, false, false);

    let iter_indices: Vec<usize> = if is_bt {
        (0..d.commits.len()).rev().collect()
    } else {
        (0..d.commits.len()).collect()
    };
    let mut placed: Vec<Option<CommitGeom>> = vec![None; d.commits.len()];
    for &i in &iter_indices {
        let c = &d.commits[i];
        let pos_with_offset = cursor + LAYOUT_OFFSET;
        let bp = branch_positions
            .iter()
            .find(|bp| bp.name == c.branch)
            .ok_or_else(|| {
                crate::error::MermaidError::Parse {
                    line: 0,
                    col: 0,
                    message: format!("commit references unknown branch '{}'", c.branch),
                }
            })?;
        let lane = bp.pos;
        let (cx, cy) = if is_vert {
            (lane, pos_with_offset)
        } else {
            (pos_with_offset, lane - 2.0)
        };
        placed[i] = Some(CommitGeom {
            id: c.id.clone(),
            seq: c.seq,
            cx,
            cy,
            pos_with_offset,
            pos: cursor,
            branch_index: bp.index,
        });
        cursor += COMMIT_STEP + LAYOUT_OFFSET;
        if cursor > max_pos {
            max_pos = cursor;
        }
    }
    for opt in placed {
        commits.push(opt.expect("every commit visited"));
    }

    // ── viewBox / bbox accumulation ─────────────────────────────────
    // Mirrors `tests/support/generate_ref.mjs` `intrinsicBox` exactly.
    let py = 2.0_f64;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut acc = |x: f64, y: f64, w: f64, h: f64| {
        if x < min_x { min_x = x; }
        if y < min_y { min_y = y; }
        if x + w > max_x { max_x = x + w; }
        if y + h > max_y { max_y = y + h; }
    };
    let rotate_pad = if d.config.rotate_commit_label { 30.0 } else { 0.0 };

    // Per-branch line spine + label background + label text.
    // `showBranches: false` short-circuits this whole section.
    if d.config.show_branches {
        for bp in &branch_positions {
            let bbox_w = bp.label_width;
            let bbox_h = bp.label_height;
            if is_vert {
                // Vertical line: x=bp.pos, y in [DEFAULT_POS, max_pos].
                acc(bp.pos, DEFAULT_POS, 0.0, max_pos - DEFAULT_POS);
                // Branch label rect: TB at top (y=0), BT at bottom (y=maxPos).
                let bkg_x = bp.pos - bbox_w / 2.0 - 10.0;
                let bkg_y = if is_bt { max_pos } else { 0.0 };
                let bkg_w = bbox_w + 18.0;
                let bkg_h = bbox_h + 4.0;
                acc(bkg_x, bkg_y, bkg_w, bkg_h);
                acc(0.0, 0.0, bbox_w, bbox_h);
            } else {
                let spine_y = bp.pos - 2.0;
                acc(0.0, spine_y, max_pos, 0.0);
                let bkg_x = -bbox_w - 4.0 - rotate_pad;
                let bkg_y = -bbox_h / 2.0 + 10.0;
                let bkg_w = bbox_w + 18.0;
                let bkg_h = bbox_h + 4.0;
                acc(bkg_x, bkg_y, bkg_w, bkg_h);
                acc(0.0, 0.0, bbox_w, bbox_h);
            }
        }
    }

    // commit-arrows: bbox of each parent → commit segment endpoints.
    // Mirrors upstream's per-arrow path. For straight segments the bbox
    // is simply the rectangle spanning (p1, p2). For curved segments
    // including arc endpoints the bbox the shim sees is also derived
    // from the path's M/L/A control points (it ignores the curve shape).
    // We approximate by accumulating each segment endpoint as a 0×0 box.
    for (i, c) in d.commits.iter().enumerate() {
        for parent_id in &c.parents {
            let parent_idx = match d.commits.iter().position(|cc| &cc.id == parent_id) {
                Some(p) => p,
                None => continue,
            };
            let p1 = &commits[parent_idx];
            let p2 = &commits[i];
            // Endpoints — covers straight + curved cases at endpoints.
            // The shim's `intrinsicBox` for paths takes (M.x, M.y, dx, dy)
            // where dx/dy = abs differences of M and last L. We need to
            // match exactly so we accumulate the rectangle spanning M
            // and the final L. Curves with intermediate arcs still end
            // at p2 so the final-segment bbox is the same as the straight
            // case for our purposes.
            let lx = p1.cx.min(p2.cx);
            let ly = p1.cy.min(p2.cy);
            let w = (p1.cx - p2.cx).abs();
            let h = (p1.cy - p2.cy).abs();
            acc(lx, ly, w, h);
        }
    }
    // commit-bullets (circles) + merge inner circle (smaller bbox is
    // subsumed by the outer r=10 box, so accumulating the outer once
    // is enough).
    for c in &commits {
        acc(c.cx - 10.0, c.cy - 10.0, 20.0, 20.0);
    }
    // commit-labels (rect + text). For merge commits without `customId`
    // upstream skips the label entirely; `showCommitLabel: false` skips
    // every label diagram-wide.
    for (i, c) in commits.iter().enumerate() {
        let commit = &d.commits[i];
        let label_emitted = d.config.show_commit_label
            && !matches!(commit.kind, CommitKind::CherryPick)
            && !(matches!(commit.kind, CommitKind::Merge) && !commit.custom_id);
        if label_emitted {
            let lw = label_widths[i];
            let lh = commit_label_text_height;
            if is_vert {
                let px = 4.0;
                let rect_x = c.cx - (lw + 4.0 * px + 5.0);
                let rect_y = c.cy - 12.0;
                acc(rect_x, rect_y, lw + 2.0 * py, lh + 2.0 * py);
                acc(0.0, 0.0, lw, lh);
            } else {
                acc(c.pos_with_offset - lw / 2.0 - py, c.cy + 13.5, lw + 2.0 * py, lh + 2.0 * py);
                acc(0.0, 0.0, lw, lh);
            }
        }
    }

    // commit-tags.
    let px = 4.0_f64;
    let tag_lh = commit_label_text_height;
    let h2 = tag_lh / 2.0;
    for (i, c) in commits.iter().enumerate() {
        let commit = &d.commits[i];
        let tags = &commit.tags;
        if tags.is_empty() {
            continue;
        }
        let mut max_w = 0.0_f64;
        for t in tags {
            let w = font_metrics::text_width(t, FONT_FAMILY, LABEL_SIZE, false, false);
            if w > max_w {
                max_w = w;
            }
        }
        let mut y_off = 0.0_f64;
        for t in tags.iter().rev() {
            if is_vert {
                // TB intrinsic bbox of the polygon (jsdom shim ignores
                // the rotate transform).
                let y_origin = c.pos + y_off;
                let lo = 10.0;
                let pad = 2.0;
                let h2_tb = tag_lh / 2.0;
                // Polygon points span x in [c.cx, c.cx + lo + max_w + 4]
                // and y in [y_origin - h2_tb - pad, y_origin + h2_tb + pad].
                let lo_x = c.cx;
                let hi_x = c.cx + lo + max_w + 4.0;
                let lo_y = y_origin - h2_tb - pad;
                let hi_y = y_origin + h2_tb + pad;
                acc(lo_x, lo_y, hi_x - lo_x, hi_y - lo_y);
                acc(c.cx + px / 2.0 - 1.5, y_origin - 1.5, 3.0, 3.0);
                let w_tag = font_metrics::text_width(t, FONT_FAMILY, LABEL_SIZE, false, false);
                acc(0.0, 0.0, w_tag, tag_lh);
            } else {
                let ly = c.cy - 19.2 - y_off;
                let p1x = c.pos - max_w / 2.0 - px / 2.0;
                let p1y = ly + py;
                let p2y = ly - py;
                let p3x = c.pos_with_offset - max_w / 2.0 - px;
                let p3y = ly - h2 - py;
                let p4x = c.pos_with_offset + max_w / 2.0 + px;
                let p5y = ly + h2 + py;
                let p6x = c.pos_with_offset - max_w / 2.0 - px;
                let lo_x = p1x.min(p3x).min(p4x).min(p6x);
                let hi_x = p1x.max(p3x).max(p4x).max(p6x);
                let lo_y = p1y.min(p2y).min(p3y).min(p5y);
                let hi_y = p1y.max(p2y).max(p3y).max(p5y);
                acc(lo_x, lo_y, hi_x - lo_x, hi_y - lo_y);
                let hole_cx = c.pos - max_w / 2.0 + px / 2.0;
                acc(hole_cx - 1.5, ly - 1.5, 3.0, 3.0);
                let w_tag = font_metrics::text_width(t, FONT_FAMILY, LABEL_SIZE, false, false);
                acc(0.0, 0.0, w_tag, tag_lh);
            }
            y_off += 20.0;
        }
    }

    drop(acc);
    let title_x = min_x + (max_x - min_x) / 2.0;

    if let Some(title) = d.meta.title.as_deref() {
        let title_w = font_metrics::text_width(title, FONT_FAMILY, LABEL_SIZE, false, false);
        if title_w > max_x { max_x = title_w; }
        if 0.0 < min_x { min_x = 0.0; }
        if label_h > max_y { max_y = label_h; }
        if 0.0 < min_y { min_y = 0.0; }
    }

    let pad = 8.0;
    let viewbox_x = min_x - pad;
    let viewbox_y = min_y - pad;
    let viewbox_w = (max_x - min_x) + 2.0 * pad;
    let viewbox_h = (max_y - min_y) + 2.0 * pad;

    Ok(GitGraphLayout {
        orientation: d.orientation,
        is_tb,
        is_bt,
        branches: branch_positions,
        commits,
        max_pos,
        viewbox_x,
        viewbox_y,
        viewbox_w,
        viewbox_h,
        branch_label_height: label_h,
        commit_label_height: label_h,
        commit_label_widths: label_widths,
        commit_label_text_height,
        title_x,
    })
}
