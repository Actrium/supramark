//! gitGraph layout — minimal subset port of upstream `gitGraphRenderer.ts`.
//!
//! Currently models only what the linear/single-branch byte-exact
//! fixtures need:
//!   - one branch lane per branch (`branchPos`)
//!   - sequential commit positions along the LR axis
//!   - geometry constants COMMIT_STEP=40, LAYOUT_OFFSET=10
//!
//! Anything fancier (rerouting, parallelCommits, TB/BT, redux/neo) is
//! deferred to follow-up work; renderer falls back to Unsupported.

use crate::error::Result;
use crate::font_metrics;
use crate::model::gitgraph::{GitGraphDiagram, Orientation};
use crate::theme::ThemeVariables;

pub const LAYOUT_OFFSET: f64 = 10.0;
pub const COMMIT_STEP: f64 = 40.0;

#[derive(Debug, Clone)]
pub struct BranchPosition {
    pub name: String,
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
}

#[derive(Debug, Clone)]
pub struct GitGraphLayout {
    pub orientation: Orientation,
    pub branches: Vec<BranchPosition>,
    pub commits: Vec<CommitGeom>,
    pub max_pos: f64,
    /// SVG viewBox parameters, computed at the end after all elements
    /// are placed. Mirrors upstream `setupGraphViewbox`.
    pub viewbox_x: f64,
    pub viewbox_y: f64,
    pub viewbox_w: f64,
    pub viewbox_h: f64,
    /// Height of the branch-label text used for vertical alignment
    /// math (rect y/transform).
    pub branch_label_height: f64,
    pub commit_label_height: f64,
    /// Actual measured widths of each commit-label, in the same order
    /// as `commits`. Used to position the per-commit `<rect>` background.
    pub commit_label_widths: Vec<f64>,
    /// Height of the commit-label text (font-size 10 sans-serif).
    pub commit_label_text_height: f64,
}

pub fn layout(d: &GitGraphDiagram, _theme: &ThemeVariables) -> Result<GitGraphLayout> {
    if d.orientation != Orientation::LR {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: TB/BT orientations not yet implemented".into(),
        ));
    }
    if d.branches.len() != 1 {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: multi-branch layouts not yet implemented".into(),
        ));
    }
    if d.commits.iter().any(|c| c.parents.len() > 1) {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: merge commits not yet implemented".into(),
        ));
    }
    if !d.config.rotate_commit_label {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: rotateCommitLabel=false not yet implemented".into(),
        ));
    }
    if d.commits.iter().any(|c| !c.tags.is_empty()) {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: commit tags not yet implemented".into(),
        ));
    }
    if d.commits
        .iter()
        .any(|c| c.kind != crate::model::gitgraph::CommitKind::Normal)
    {
        return Err(crate::error::MermaidError::Unsupported(
            "gitGraph: REVERSE/HIGHLIGHT commit types not yet implemented".into(),
        ));
    }

    // Branch label text — measured at the moment of `getBBox()` on the
    // freshly-inserted `<text>` element. The diagram svg's `<style>`
    // block isn't authoritative for jsdom (it doesn't run CSS), so the
    // font defaults to sans-serif/14px until an attribute on the text
    // (or an ancestor) overrides them. None do — so we always measure
    // at 14px sans-serif, matching the reference shim.
    let font_family = "sans-serif";
    let label_size = 14.0_f64;
    let main_branch = &d.branches[0];
    let label_w = font_metrics::text_width(&main_branch.name, font_family, label_size, false, false);
    let label_h = font_metrics::line_height(font_family, label_size, false, false);
    // For "main" with default font: label_h ≈ 18.4. Reference uses 20.296875 for
    // the rect height; that's bbox.height + 4 + 0 (LR-non-redux). Need height
    // matching upstream's getBBox (which returns *line height of tspan*).

    let branch = BranchPosition {
        name: main_branch.name.clone(),
        pos: 0.0,
        index: 0,
        label_width: label_w,
        label_height: label_h,
    };

    // Commits along LR — positions match upstream's `drawCommits` linear path.
    let mut commits = Vec::with_capacity(d.commits.len());
    let mut pos: f64 = 0.0;
    let mut max_pos: f64 = 0.0;
    // Commit labels are measured under the same jsdom defaults as the
    // branch label — 14px sans-serif. The CSS class `commit-label` does
    // set `font-size:10px` but that's text-rendering-only; the bbox
    // shim doesn't apply CSS, so font-size stays at 14.
    let label_widths: Vec<f64> = d
        .commits
        .iter()
        .map(|c| font_metrics::text_width(&c.id, font_family, label_size, false, false))
        .collect();
    let commit_label_text_height =
        font_metrics::line_height(font_family, label_size, false, false);

    for c in &d.commits {
        let pos_with_offset = pos + LAYOUT_OFFSET;
        let cx = pos_with_offset;
        let cy = -2.0; // LR non-redux: branchY (=0) + (-2)
        commits.push(CommitGeom {
            id: c.id.clone(),
            seq: c.seq,
            cx,
            cy,
            pos_with_offset,
            pos,
        });
        pos += COMMIT_STEP + LAYOUT_OFFSET;
        if pos > max_pos {
            max_pos = pos;
        }
    }
    // Upstream's drawBranches uses `maxPos` AFTER the loop without the trailing
    // increment? Actually no — the increment happens unconditionally at end of
    // loop body, so for N commits with COMMIT_STEP=40 + LAYOUT_OFFSET=10, max_pos
    // ends at N*50. For N=3: max_pos=150. That matches the reference where the
    // branch line goes x1=0 → x2=150.

    // Compute viewBox. Upstream's setupGraphViewbox takes the bbox of the
    // diagram <g> and adds padding. Reference 01:
    //   viewBox="-76.9794921875 -20 234.9794921875 59.796875"
    //   max-width: 234.9794921875
    // The bbox spans:
    //   left = -bbox_width(main label) - 18 (label rect x value)
    //          actually: branch-label rect has x = -bbox.width - 4 - (rotateCommitLabel?30:0)
    //          and a transform translate(-19, ...). With rotateCommitLabel=true
    //          its left edge is at -bbox_w - 4 - 30 - 19 = -bbox_w - 53.
    //          For "main" bbox_w=24..., that gives -77 ish — matches.
    //   right = max_pos (line x2)
    //   top = the smallest y across all elements (branch label rect top, commit
    //         labels above-the-line, etc.)
    //   bottom = the largest y across all elements (commit labels below the
    //            line + their rotated bbox extends).
    // The reference viewBox values were produced by jsdom's getBBox; our
    // font-metrics give us the same numeric values for byte-exact parity.
    let bbox_w = label_w; // text bbox.width in upstream
    let bbox_h = label_h; // text bbox.height
    // branch label rect (LR, non-redux): x = -bbox_w - 4 - 30, y = -bbox_h/2 + 10
    // width = bbox_w + 18 + 0; height = bbox_h + 4
    // Then transform: translate(-19, spineY - 12 - 0) where spineY = -2
    // So actual visual rect: x = -bbox_w-34-19 = -bbox_w-53, y = ... + (spineY-12) = ... + -14
    //                        width = bbox_w + 18; height = bbox_h + 4.
    let rect_left = -bbox_w - 53.0; // visual left edge of branch-label background
    let line_right = max_pos;
    // Padding from `setupGraphViewbox` is `gitGraphConfig.diagramPadding` (= 8 default).
    // But the reference 01 has padding 8 already baked: viewBox left = -76.97 vs rect_left.
    // For "main", bbox_w = 24.something from font metrics; -bbox_w-53 ≈ -77.
    // The viewBox actually adds NO extra padding on the left in this fixture
    // because the viewBox is computed from the inner <g> bbox + padding,
    // and the branchLabel rect already extends past the SVG's x=0 anchor.
    // We approximate: viewBox_x = rect_left, w = line_right - rect_left, etc.

    // For commit labels (rotated -45°), each commit-label group is at
    // commit cx, with rect centred at (cx) horizontally then translated by
    // (translate_x, translate_y) and rotated. The rotated bbox extends below
    // the line by ~20px and to the upper-right by some, but since we only need
    // viewBox numbers for byte-exact, we hard-derive from the reference once
    // by computing the bbox of all elements.

    // ── Compute commit-label rotated bbox extents ─────────────────────────
    // Each label group: <g transform="translate(tx, ty) rotate(-45, cx, cy)">
    //   <rect x=cx_local y=11.5 width=rect_w height=rect_h>
    //   <text x=tx_text y=23>
    // tx = -7.5 - ((rect_w + 10) / 25) * 9.5  ... but this is wrapper transform.
    //
    // For simplicity and because we want byte-exact numbers, replicate
    // upstream's drawCommitLabel + jsdom getBBox by computing what the
    // visual extents WILL BE after the rotation. This is non-trivial.
    //
    // Empirically (verified against fixture 01):
    //   viewBox y_min = -20
    //   viewBox y_max = 39.796875  → height = 59.796875
    // The 39.796875 number = 13.02845703125 (the wrapper translate y) +
    //   rect bottom of post-rotate bbox.
    // Pull these from a known formula: the rotated rect has corners at
    // (x, y), (x+w, y), (x+w, y+h), (x, y+h) before rotation. After rotating
    // -45° around (cx, cy), the y-extent depends on the rect's centre offset.
    //
    // Simpler: we derive the global bbox by simulating transform of every
    // anchor point.

    // Branch-label rect — already accounted for above (top at spineY-14 =
    // -16, height bbox_h+4 ≈ 22 → bottom ≈ +6).
    // top_branch = -16
    // bottom_branch = -16 + (bbox_h + 4)

    // Commit-label rotation.
    // We mirror upstream:
    //   bbox_text.width = label_w_i (font_metrics), bbox_text.height = font_metrics line height.
    //   labelBkg attrs (after `text.attr('x',...)` second pass): x = posWithOffset - bbox.width/2 - PY,
    //     y = commitPosition.y + 13.5, w = bbox.width + 2*PY, h = bbox.height + 2*PY
    //   text attr: x = posWithOffset - bbox.width/2, y = commitPosition.y + 25, font 10px
    // Then wrapper g gets transform `translate(r_x, r_y) rotate(-45, pos, cy)` where
    //   r_x = -7.5 - ((bbox.width + 10) / 25) * 9.5
    //   r_y = 10 + (bbox.width / 25) * 8.5
    // With pos = posWithOffset - LAYOUT_OFFSET (= cx - 10), cy = -2.
    //
    // We'll just compute the rotated bbox of the wrapper's combined rect+text.

    // Compute the diagram bbox the way jsdom's getBBox shim does — by
    // unioning each rendered element's intrinsic box (without applying
    // transforms). Mirrors `tests/support/generate_ref.mjs` `intrinsicBox`
    // exactly, since the reference SVGs are produced through that shim.
    //
    // Elements rendered by the renderer:
    //   - line spine: x1=0..max_pos, y=-2 → bbox (0, -2, max_pos, 0)
    //   - branch label rect: (rect_x, rect_y, rect_w, rect_h)
    //   - branch label `<text>`: shim returns (0, 0, text_w, text_h)
    //     — we still include this because mermaid renders one.
    //   - commit bullets <circle cx, cy, r=10>: (cx-10, cy-10, 20, 20)
    //   - commit-arrow <path d="M..L..">: bbox of segment endpoints
    //   - commit-label rect at (cx-lw/2-py, cy+13.5, lw+2py, lh+2py)
    //   - commit-label <text>: (0, 0, lw, lh)
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
    // Empty commit-bullet pre-pass <g></g>s contribute nothing (no rendered
    // children of their own with bbox-relevant attrs).
    // Initial empty placeholders (commit-bullets/commit-labels first pass)
    // each have NO children → no contribution.
    //
    // The third <g> (branch group) has line + rect + text.
    // line spine
    acc(0.0, -2.0, max_pos, 0.0);
    // branch label rect (x,y,w,h)
    let bkg_x = -bbox_w - 4.0 - 30.0;
    let bkg_y = -bbox_h / 2.0 + 10.0;
    let bkg_w = bbox_w + 18.0;
    let bkg_h = bbox_h + 4.0;
    acc(bkg_x, bkg_y, bkg_w, bkg_h);
    // branch label text (intrinsic 0,0,w,h)
    acc(0.0, 0.0, bbox_w, bbox_h);

    // commit-arrows
    for win in commits.windows(2) {
        let a = &win[0];
        let b = &win[1];
        let lx = a.cx.min(b.cx);
        let ly = a.cy.min(b.cy);
        let w = (a.cx - b.cx).abs();
        let h = (a.cy - b.cy).abs();
        acc(lx, ly, w, h);
    }
    // commit-bullets (circles)
    for c in &commits {
        acc(c.cx - 10.0, c.cy - 10.0, 20.0, 20.0);
    }
    // commit-labels (rect + text). All rotated, but jsdom's shim doesn't
    // apply transforms. Each commit's rect intrinsic bbox is at
    // (cx - lw/2 - py, cy + 13.5, lw+2py, lh+2py), and the text intrinsic
    // is (0, 0, lw, lh).
    for (i, c) in commits.iter().enumerate() {
        let lw = label_widths[i];
        let lh = commit_label_text_height;
        acc(c.pos_with_offset - lw / 2.0 - py, c.cy + 13.5, lw + 2.0 * py, lh + 2.0 * py);
        acc(0.0, 0.0, lw, lh);
    }

    let pad = 8.0;
    let viewbox_x = min_x - pad;
    let viewbox_y = min_y - pad;
    let viewbox_w = (max_x - min_x) + 2.0 * pad;
    let viewbox_h = (max_y - min_y) + 2.0 * pad;

    Ok(GitGraphLayout {
        orientation: d.orientation,
        branches: vec![branch],
        commits,
        max_pos,
        viewbox_x,
        viewbox_y,
        viewbox_w,
        viewbox_h,
        branch_label_height: label_h,
        commit_label_height: bbox_h,
        commit_label_widths: label_widths,
        commit_label_text_height,
    })
}
