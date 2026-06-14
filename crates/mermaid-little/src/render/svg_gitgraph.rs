//! gitGraph SVG renderer — port of upstream `gitGraphRenderer.ts` LR mode.
//!
//! Targets byte-exact parity with mermaid@11.14.0 for:
//!   - linear, single-branch fixtures
//!   - multi-branch (per-lane spine + label)
//!   - `merge` commits (double-circle bullet, curved cross-lane arrow)
//!   - `tag:`, `type:` (REVERSE/HIGHLIGHT) modifiers, `commit-label`
//!
//! Out of scope (rejected at layout stage): TB/BT, cherry-pick,
//! parallelCommits, redux geometry / non-default themes.

use crate::error::Result;
use crate::layout::gitgraph::GitGraphLayout;
use crate::model::gitgraph::{CommitKind, GitGraphDiagram};
use crate::theme::ThemeVariables;

/// Mirror upstream `calcColorIndex(rawIndex, THEME_COLOR_LIMIT=8, useColorTheme=false)`.
/// Default theme always falls into the simple `rawIndex % 8` branch.
#[inline]
fn color_idx(raw: usize) -> usize {
    raw % 8
}

pub fn render(
    d: &GitGraphDiagram,
    l: &GitGraphLayout,
    theme: &ThemeVariables,
    id: &str,
) -> Result<String> {
    let mut out = String::with_capacity(8192);

    // ── Opening <svg> ────────────────────────────────────────────────
    let vb = format!(
        "{} {} {} {}",
        fmt_num(l.viewbox_x),
        fmt_num(l.viewbox_y),
        fmt_num(l.viewbox_w),
        fmt_num(l.viewbox_h),
    );
    out.push_str(&format!(
        r#"<svg id="{id}" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: {w}px;" viewBox="{vb}" role="graphics-document document" aria-roledescription="gitGraph">"#,
        id = id,
        vb = vb,
        w = fmt_num(l.viewbox_w),
    ));

    // ── <style> ──────────────────────────────────────────────────────
    out.push_str(&style_block(id, theme));

    // ── First-pass placeholder groups (jsdom artefact) ───────────────
    // Upstream emits `<g></g>` once (the temporary measurement group),
    // followed by empty `<g class="commit-bullets"></g>` and
    // `<g class="commit-labels"></g>` from the first `drawCommits(false)` pass.
    out.push_str("<g></g>");
    out.push_str(r#"<g class="commit-bullets"></g>"#);
    out.push_str(r#"<g class="commit-labels"></g>"#);

    // ── Branch group: line + label background + label per branch ─────
    // `showBranches: false` skips the entire group entirely (mirroring
    // upstream `if (gitGraphConfig.showBranches) drawBranches(...)`).
    if d.config.show_branches {
        out.push_str("<g>");
        for bp in &l.branches {
            let bbox_w = bp.label_width;
            let bbox_h = bp.label_height;
            let rotate_pad = if d.config.rotate_commit_label {
                30.0
            } else {
                0.0
            };
            let cidx = color_idx(bp.index);

            if l.is_tb || l.is_bt {
                // Vertical line: x=pos, y between DEFAULT_POS and max_pos.
                // BT swaps the line's y1/y2 endpoints (y1=max_pos, y2=DEFAULT_POS).
                let line_x = bp.pos;
                let (line_y1, line_y2) = if l.is_bt {
                    (l.max_pos, crate::layout::gitgraph::DEFAULT_POS)
                } else {
                    (crate::layout::gitgraph::DEFAULT_POS, l.max_pos)
                };
                // Branch label rect: TB → top (y=0), BT → bottom (y=maxPos).
                let bkg_x = bp.pos - bbox_w / 2.0 - 10.0;
                let bkg_y = if l.is_bt { l.max_pos } else { 0.0 };
                let bkg_w = bbox_w + 18.0;
                let bkg_h = bbox_h + 4.0;
                let label_translate_x = bp.pos - bbox_w / 2.0 - 5.0;
                let label_translate_y = bkg_y;
                out.push_str(&format!(
                r#"<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" class="branch branch{idx}"></line>"#,
                x = fmt_num(line_x),
                y1 = fmt_num(line_y1),
                y2 = fmt_num(line_y2),
                idx = cidx,
            ));
                out.push_str(&format!(
                r#"<rect class="branchLabelBkg label{idx}" style="" rx="4" ry="4" x="{x}" y="{y}" width="{w}" height="{h}"></rect>"#,
                idx = cidx,
                x = fmt_num(bkg_x),
                y = fmt_num(bkg_y),
                w = fmt_num(bkg_w),
                h = fmt_num(bkg_h),
            ));
                out.push_str(&format!(
                r#"<g class="branchLabel"><g class="label branch-label{idx}" transform="translate({tx}, {ty})"><text>{tspans}</text></g></g>"#,
                idx = cidx,
                tx = fmt_num(label_translate_x),
                ty = fmt_num(label_translate_y),
                tspans = render_branch_tspans(&bp.name),
            ));
            } else {
                let bkg_x = -bbox_w - 4.0 - rotate_pad;
                let bkg_y = -bbox_h / 2.0 + 10.0;
                let bkg_w = bbox_w + 18.0;
                let bkg_h = bbox_h + 4.0;
                let spine_y = bp.pos - 2.0;
                let label_translate_x = -bbox_w - 14.0 - rotate_pad;
                let label_translate_y = spine_y - bbox_h / 2.0 - 2.0;
                let bkg_translate_x = -19.0;
                let bkg_translate_y = spine_y - 12.0;

                out.push_str(&format!(
                r#"<line x1="0" y1="{sy}" x2="{maxp}" y2="{sy}" class="branch branch{idx}"></line>"#,
                sy = fmt_num(spine_y),
                maxp = fmt_num(l.max_pos),
                idx = cidx,
            ));
                out.push_str(&format!(
                r#"<rect class="branchLabelBkg label{idx}" style="" rx="4" ry="4" x="{x}" y="{y}" width="{w}" height="{h}" transform="translate({tx}, {ty})"></rect>"#,
                idx = cidx,
                x = fmt_num(bkg_x),
                y = fmt_num(bkg_y),
                w = fmt_num(bkg_w),
                h = fmt_num(bkg_h),
                tx = fmt_num(bkg_translate_x),
                ty = fmt_num(bkg_translate_y),
            ));
                out.push_str(&format!(
                r#"<g class="branchLabel"><g class="label branch-label{idx}" transform="translate({tx}, {ty})"><text>{tspans}</text></g></g>"#,
                idx = cidx,
                tx = fmt_num(label_translate_x),
                ty = fmt_num(label_translate_y),
                tspans = render_branch_tspans(&bp.name),
            ));
            }
        }
        out.push_str("</g>");
    }

    // ── Arrows: walk commits in chronological order; for each commit
    //    emit one path per parent edge. Mirrors upstream `drawArrows`.
    //
    // `lanes` accumulates Y values used by `findLane` — initially seeded
    // with each visible branch's spine Y, then mutated as we route.
    out.push_str(r#"<g class="commit-arrows">"#);
    // `lanes` seed: in LR each spine y = bp.pos - 2; in TB each lane x
    // = bp.pos. Mirrors upstream's `lanes.push(spineY)` which uses the
    // same coordinate as the line's primary axis.
    let mut lanes: Vec<f64> = if d.config.show_branches {
        if l.is_tb || l.is_bt {
            l.branches.iter().map(|bp| bp.pos).collect()
        } else {
            l.branches.iter().map(|bp| bp.pos - 2.0).collect()
        }
    } else {
        Vec::new()
    };
    for (i, c) in d.commits.iter().enumerate() {
        for parent_id in &c.parents {
            let parent_idx = match d.commits.iter().position(|cc| &cc.id == parent_id) {
                Some(p) => p,
                None => continue,
            };
            let pa = &l.commits[parent_idx];
            let pb = &l.commits[i];
            let parent_commit = &d.commits[parent_idx];
            let is_v = l.is_tb || l.is_bt;
            let needs_reroute = should_reroute(parent_commit, c, pa, pb, &d.commits, is_v);
            let line_def = if needs_reroute {
                build_arrow_path_rerouted(pa, pb, c, parent_commit, &mut lanes, is_v, l.is_bt)
            } else if l.is_tb {
                build_arrow_path_tb(pa, pb, c, parent_commit)
            } else if l.is_bt {
                build_arrow_path_bt(pa, pb, c, parent_commit)
            } else {
                build_arrow_path(pa, pb, c, parent_commit)
            };
            // Color class — see upstream `drawArrow` for the rules:
            // - for non-merge edges: dest branch color
            // - for merge edges where `commitA.id !== commitB.parents[0]`:
            //   source branch color
            // Color rule (mirrors upstream `drawArrow`):
            //   - default: destination branch color
            //   - merge with non-primary parent: source branch color
            //   - rerouted with source-on-bottom-side: source-branch
            //     color (the override at upstream line 734 / 704).
            //   - TB non-rerouted with source.x > dest.x AND merge
            //     non-primary: source-branch color (line 704).
            let is_merge_2nd =
                matches!(c.kind, CommitKind::Merge) && c.parents.first() != Some(parent_id);
            let reroute_axis_swap = if l.is_tb || l.is_bt {
                pa.cx > pb.cx
            } else {
                pa.cy > pb.cy
            };
            let raw_idx = if is_merge_2nd {
                pa.branch_index
            } else if needs_reroute && reroute_axis_swap {
                pa.branch_index
            } else {
                pb.branch_index
            };
            out.push_str(&format!(
                r#"<path d="{d}" class="arrow arrow{idx}"></path>"#,
                d = line_def,
                idx = color_idx(raw_idx),
            ));
        }
    }
    out.push_str("</g>");

    // ── Commit bullets ───────────────────────────────────────────────
    // BT iterates the sortedKeys in reverse (upstream line 588), so the
    // emit order is newest-first. Mirror that by walking indices in
    // reverse for BT.
    out.push_str(r#"<g class="commit-bullets">"#);
    let bullet_order: Vec<usize> = if l.is_bt {
        (0..d.commits.len()).rev().collect()
    } else {
        (0..d.commits.len()).collect()
    };
    for &i in &bullet_order {
        let c = &l.commits[i];
        let commit = &d.commits[i];
        let id_esc = escape_text(&commit.id);
        // Effective symbol type: `commit.customType ?? commit.type`.
        let symbol = commit.custom_type.unwrap_or(commit.kind);
        // typeClass mirrors `getCommitClassType` — derived from the
        // effective symbol so a `merge ... type: REVERSE` emits
        // `commit-reverse` rather than `commit-merge` on the cross path.
        let type_class = symbol.class();
        let cidx = color_idx(c.branch_index);
        match symbol {
            CommitKind::Highlight => {
                let ox = c.cx - 10.0;
                let oy = c.cy - 10.0;
                let ix = c.cx - 6.0;
                let iy = c.cy - 6.0;
                out.push_str(&format!(
                    r#"<rect x="{ox}" y="{oy}" width="20" height="20" class="commit {id} commit-highlight{cidx} {tc}-outer"></rect><rect x="{ix}" y="{iy}" width="12" height="12" class="commit {id} commit{cidx} {tc}-inner"></rect>"#,
                    ox = fmt_num(ox),
                    oy = fmt_num(oy),
                    ix = fmt_num(ix),
                    iy = fmt_num(iy),
                    id = id_esc,
                    tc = type_class,
                    cidx = cidx,
                ));
            }
            CommitKind::CherryPick => {
                // Outer circle r=10 + two filled white "splatter" dots
                // at (cx±3, cy+2) r=2.75 + two short white lines
                // forming a wedge (cx+3, cy+1) → (cx, cy-5) and
                // (cx-3, cy+1) → (cx, cy-5). Mirrors upstream
                // `drawCommitBullet` cherry-pick branch (light theme).
                let hash = "#";
                out.push_str(&format!(
                    r#"<circle cx="{cx}" cy="{cy}" r="10" class="commit {id} {tc}"></circle><circle cx="{cx1}" cy="{cy1}" r="2.75" fill="{h}fff" class="commit {id} {tc}"></circle><circle cx="{cx2}" cy="{cy1}" r="2.75" fill="{h}fff" class="commit {id} {tc}"></circle><line x1="{lx1}" y1="{ly1}" x2="{lx2}" y2="{ly2}" stroke="{h}fff" class="commit {id} {tc}"></line><line x1="{lx3}" y1="{ly1}" x2="{lx2}" y2="{ly2}" stroke="{h}fff" class="commit {id} {tc}"></line>"#,
                    cx = fmt_num(c.cx),
                    cy = fmt_num(c.cy),
                    cx1 = fmt_num(c.cx - 3.0),
                    cx2 = fmt_num(c.cx + 3.0),
                    cy1 = fmt_num(c.cy + 2.0),
                    lx1 = fmt_num(c.cx + 3.0),
                    ly1 = fmt_num(c.cy + 1.0),
                    lx2 = fmt_num(c.cx),
                    ly2 = fmt_num(c.cy - 5.0),
                    lx3 = fmt_num(c.cx - 3.0),
                    id = id_esc,
                    tc = type_class,
                    h = hash,
                ));
            }
            _ => {
                out.push_str(&format!(
                    r#"<circle cx="{cx}" cy="{cy}" r="10" class="commit {id} commit{cidx}"></circle>"#,
                    cx = fmt_num(c.cx),
                    cy = fmt_num(c.cy),
                    id = id_esc,
                    cidx = cidx,
                ));
                if matches!(symbol, CommitKind::Merge) {
                    out.push_str(&format!(
                        r#"<circle cx="{cx}" cy="{cy}" r="6" class="commit {tc} {id} commit{cidx}"></circle>"#,
                        cx = fmt_num(c.cx),
                        cy = fmt_num(c.cy),
                        id = id_esc,
                        tc = type_class,
                        cidx = cidx,
                    ));
                }
                if matches!(symbol, CommitKind::Reverse) {
                    let cv = 5.0_f64;
                    out.push_str(&format!(
                        r#"<path d="M {x1},{y1}L{x2},{y2}M{x1b},{y2b}L{x2b},{y1b}" class="commit {tc} {id} commit{cidx}"></path>"#,
                        x1 = fmt_num(c.cx - cv),
                        y1 = fmt_num(c.cy - cv),
                        x2 = fmt_num(c.cx + cv),
                        y2 = fmt_num(c.cy + cv),
                        x1b = fmt_num(c.cx - cv),
                        y2b = fmt_num(c.cy + cv),
                        x2b = fmt_num(c.cx + cv),
                        y1b = fmt_num(c.cy - cv),
                        tc = type_class,
                        id = id_esc,
                        cidx = cidx,
                    ));
                }
            }
        }
    }
    out.push_str("</g>");

    // ── Commit labels (interleaved with tags) ───────────────────────
    out.push_str(r#"<g class="commit-labels">"#);
    let py = 2.0_f64;
    let px = 4.0_f64;
    let tag_lh = l.commit_label_text_height;
    let h2 = tag_lh / 2.0;
    for &i in &bullet_order {
        let c = &l.commits[i];
        let commit = &d.commits[i];
        // Skip the commit-LABEL (text + bkg) for cherry-pick and
        // non-customId merge, plus when `showCommitLabel` is off — but
        // always emit the tag(s), regardless of these label suppressions.
        let skip_label = matches!(commit.kind, CommitKind::CherryPick)
            || (matches!(commit.kind, CommitKind::Merge) && !commit.custom_id)
            || !d.config.show_commit_label;
        if !skip_label {
            let lw = l.commit_label_widths[i];
            let lh = l.commit_label_text_height;
            if l.is_tb || l.is_bt {
                // TB layout (upstream lines 360-368):
                //   labelBkg.x = cx - (bbox.w + 4*PX + 5)
                //          .y = cy - 12
                //          .w = bbox.w + 2*PY (set in the LR pre-block,
                //                              still the same value)
                //          .h = bbox.h + 2*PY
                //   text.x = cx - (bbox.w + 4*PX)
                //   text.y = cy + bbox.h - 12
                let px = 4.0_f64;
                let rect_x = c.cx - (lw + 4.0 * px + 5.0);
                let rect_y = c.cy - 12.0;
                let rect_w = lw + 2.0 * py;
                let rect_h = lh + 2.0 * py;
                let text_x = c.cx - (lw + 4.0 * px);
                let text_y = c.cy + lh - 12.0;
                if d.config.rotate_commit_label {
                    // TB+rotate: text + bkg each get a `rotate(-45, cx, cy)`
                    // (NOT a wrapper transform — see upstream lines 372-380).
                    out.push_str(&format!(
                        r#"<g><rect class="commit-label-bkg" x="{x}" y="{y}" width="{w}" height="{h}" transform="rotate(-45, {cx}, {cy})"></rect><text x="{tx}" y="{ty}" class="commit-label" transform="rotate(-45, {cx}, {cy})">{label}</text></g>"#,
                        x = fmt_num(rect_x),
                        y = fmt_num(rect_y),
                        w = fmt_num(rect_w),
                        h = fmt_num(rect_h),
                        tx = fmt_num(text_x),
                        ty = fmt_num(text_y),
                        cx = fmt_num(c.cx),
                        cy = fmt_num(c.cy),
                        label = escape_text(&commit.id),
                    ));
                } else {
                    out.push_str(&format!(
                        r#"<g><rect class="commit-label-bkg" x="{x}" y="{y}" width="{w}" height="{h}"></rect><text x="{tx}" y="{ty}" class="commit-label">{label}</text></g>"#,
                        x = fmt_num(rect_x),
                        y = fmt_num(rect_y),
                        w = fmt_num(rect_w),
                        h = fmt_num(rect_h),
                        tx = fmt_num(text_x),
                        ty = fmt_num(text_y),
                        label = escape_text(&commit.id),
                    ));
                }
            } else {
                let rect_x = c.pos_with_offset - lw / 2.0 - py;
                let rect_y = c.cy + 13.5;
                let rect_w = lw + 2.0 * py;
                let rect_h = lh + 2.0 * py;
                let text_x = c.pos_with_offset - lw / 2.0;
                let text_y = c.cy + 25.0;
                if d.config.rotate_commit_label {
                    let r_x = -7.5 - ((lw + 10.0) / 25.0) * 9.5;
                    let r_y = 10.0 + (lw / 25.0) * 8.5;
                    out.push_str(&format!(
                        r#"<g transform="translate({rx}, {ry}) rotate(-45, {pos}, {cy})"><rect class="commit-label-bkg" x="{x}" y="{y}" width="{w}" height="{h}"></rect><text x="{tx}" y="{ty}" class="commit-label">{label}</text></g>"#,
                        rx = fmt_num(r_x),
                        ry = fmt_num(r_y),
                        pos = fmt_num(c.pos),
                        cy = fmt_num(c.cy),
                        x = fmt_num(rect_x),
                        y = fmt_num(rect_y),
                        w = fmt_num(rect_w),
                        h = fmt_num(rect_h),
                        tx = fmt_num(text_x),
                        ty = fmt_num(text_y),
                        label = escape_text(&commit.id),
                    ));
                } else {
                    out.push_str(&format!(
                        r#"<g><rect class="commit-label-bkg" x="{x}" y="{y}" width="{w}" height="{h}"></rect><text x="{tx}" y="{ty}" class="commit-label">{label}</text></g>"#,
                        x = fmt_num(rect_x),
                        y = fmt_num(rect_y),
                        w = fmt_num(rect_w),
                        h = fmt_num(rect_h),
                        tx = fmt_num(text_x),
                        ty = fmt_num(text_y),
                        label = escape_text(&commit.id),
                    ));
                }
            }
        }

        if !commit.tags.is_empty() {
            let mut max_w = 0.0_f64;
            for t in &commit.tags {
                let w = crate::font_metrics::text_width(t, "sans-serif", 14.0, false, false);
                if w > max_w {
                    max_w = w;
                }
            }
            let mut y_off = 0.0_f64;
            for t in commit.tags.iter().rev() {
                let w_tag = crate::font_metrics::text_width(t, "sans-serif", 14.0, false, false);
                if l.is_tb || l.is_bt {
                    // TB tag geometry — see upstream lines 464-487.
                    // commitPosition.x = c.cx, pos = c.pos (= cy - 10),
                    // yOrigin = pos + yOffset.
                    let y_origin = c.pos + y_off;
                    let lo = 10.0_f64; // LAYOUT_OFFSET
                    let pad = 2.0_f64;
                    let h2_tb = tag_lh / 2.0;
                    // Polygon points with leading newline-space prefix
                    // matching upstream's template literal (4 spaces + 4
                    // spaces of body indent so 8 leading spaces total per
                    // line break).
                    let points = format!(
                        "\n        {p1x},{p1y}\n        {p2x},{p2y}\n        {p3x},{p3y}\n        {p4x},{p4y}\n        {p5x},{p5y}\n        {p6x},{p6y}",
                        p1x = fmt_num(c.cx),
                        p1y = fmt_num(y_origin + pad),
                        p2x = fmt_num(c.cx),
                        p2y = fmt_num(y_origin - pad),
                        p3x = fmt_num(c.cx + lo),
                        p3y = fmt_num(y_origin - h2_tb - pad),
                        p4x = fmt_num(c.cx + lo + max_w + 4.0),
                        p4y = fmt_num(y_origin - h2_tb - pad),
                        p5x = fmt_num(c.cx + lo + max_w + 4.0),
                        p5y = fmt_num(y_origin + h2_tb + pad),
                        p6x = fmt_num(c.cx + lo),
                        p6y = fmt_num(y_origin + h2_tb + pad),
                    );
                    let transform = format!(
                        "translate(12,12) rotate(45, {},{})",
                        fmt_num(c.cx),
                        fmt_num(c.pos),
                    );
                    out.push_str(&format!(
                        r#"<polygon class="tag-label-bkg" points="{p}" transform="{tr}"></polygon>"#,
                        p = points, tr = transform,
                    ));
                    out.push_str(&format!(
                        r#"<circle cy="{cy}" cx="{cx}" r="1.5" class="tag-hole" transform="{tr}"></circle>"#,
                        cy = fmt_num(y_origin),
                        cx = fmt_num(c.cx + px / 2.0),
                        tr = transform,
                    ));
                    let tag_translate = format!(
                        "translate(14,14) rotate(45, {},{})",
                        fmt_num(c.cx),
                        fmt_num(c.pos),
                    );
                    out.push_str(&format!(
                        r#"<text y="{ty}" class="tag-label" x="{tx}" transform="{tr}">{label}</text>"#,
                        // upstream first sets `y = c.cy - 16 - yOff`, then in the TB block
                        // overrides `y = yOrigin + 3`.
                        ty = fmt_num(y_origin + 3.0),
                        tx = fmt_num(c.cx + 5.0),
                        tr = tag_translate,
                        label = escape_text(t),
                    ));
                    let _ = w_tag;
                } else {
                    let ly = c.cy - 19.2 - y_off;
                    let pwo = c.pos_with_offset;
                    let pos = c.pos;
                    let points = format!(
                        "\n      {p1x},{p1y}  \n      {p1x},{p2y}\n      {p3x},{p3y}\n      {p4x},{p3y}\n      {p4x},{p5y}\n      {p6x},{p5y}",
                        p1x = fmt_num(pos - max_w / 2.0 - px / 2.0),
                        p1y = fmt_num(ly + py),
                        p2y = fmt_num(ly - py),
                        p3x = fmt_num(pwo - max_w / 2.0 - px),
                        p3y = fmt_num(ly - h2 - py),
                        p4x = fmt_num(pwo + max_w / 2.0 + px),
                        p5y = fmt_num(ly + h2 + py),
                        p6x = fmt_num(pwo - max_w / 2.0 - px),
                    );
                    let tx = pwo - w_tag / 2.0;
                    let hole_cx = pos - max_w / 2.0 + px / 2.0;
                    out.push_str(&format!(
                        r#"<polygon class="tag-label-bkg" points="{p}"></polygon>"#,
                        p = points,
                    ));
                    out.push_str(&format!(
                        r#"<circle cy="{cy}" cx="{cx}" r="1.5" class="tag-hole"></circle>"#,
                        cy = fmt_num(ly),
                        cx = fmt_num(hole_cx),
                    ));
                    out.push_str(&format!(
                        r#"<text y="{ty}" class="tag-label" x="{tx}">{label}</text>"#,
                        ty = fmt_num(c.cy - 16.0 - y_off),
                        tx = fmt_num(tx),
                        label = escape_text(t),
                    ));
                }
                y_off += 20.0;
            }
        }
    }
    out.push_str("</g>");

    // ── Title (gitTitleText) ─────────────────────────────────────────
    if let Some(title) = d.meta.title.as_deref() {
        if !title.is_empty() {
            out.push_str(&format!(
                r#"<text text-anchor="middle" x="{x}" y="-25" class="gitTitleText">{t}</text>"#,
                x = fmt_num(l.title_x),
                t = escape_text(title),
            ));
        }
    }

    out.push_str("</svg>");
    Ok(out)
}

/// Mirror upstream `shouldRerouteArrow`. Returns `true` when there is
/// at least one commit between A and B (by `seq`) on the "outer"
/// branch — `commitB.branch` if "B is furthest forward" along the
/// running axis, else `commitA.branch`. The running axis is X for
/// TB/BT and Y for LR.
fn should_reroute(
    commit_a: &crate::model::gitgraph::Commit,
    commit_b: &crate::model::gitgraph::Commit,
    p1: &crate::layout::gitgraph::CommitGeom,
    p2: &crate::layout::gitgraph::CommitGeom,
    all_commits: &[crate::model::gitgraph::Commit],
    is_tb: bool,
) -> bool {
    let commit_b_is_furthest = if is_tb { p1.cx < p2.cx } else { p1.cy < p2.cy };
    let branch_to_get_curve = if commit_b_is_furthest {
        &commit_b.branch
    } else {
        &commit_a.branch
    };
    let lo = commit_a.seq.min(commit_b.seq);
    let hi = commit_a.seq.max(commit_b.seq);
    all_commits
        .iter()
        .any(|x| x.seq > lo && x.seq < hi && &x.branch == branch_to_get_curve)
}

/// Mirror upstream `findLane` — pick a y in the gap between `y1` and
/// `y2` that is at least 10 away from any existing lane; mutates lanes.
fn find_lane(y1: f64, y2: f64, lanes: &mut Vec<f64>) -> f64 {
    let mut hi = y2;
    for depth in 0..=5 {
        let candidate = y1 + (y1 - hi).abs() / 2.0;
        if depth == 5 {
            return candidate;
        }
        let ok = lanes.iter().all(|lane| (*lane - candidate).abs() >= 10.0);
        if ok {
            lanes.push(candidate);
            return candidate;
        }
        let diff = (y1 - hi).abs();
        hi -= diff / 5.0;
    }
    unreachable!()
}

/// Rerouted arrow — mirrors upstream `drawArrow` when
/// `arrowNeedsRerouting`. Uses two 10×10 arc detours through a
/// dynamically-allocated lane in the inter-branch gap. TB pivots the
/// detour onto the X axis; LR onto the Y axis.
fn build_arrow_path_rerouted(
    pa: &crate::layout::gitgraph::CommitGeom,
    pb: &crate::layout::gitgraph::CommitGeom,
    _commit_b: &crate::model::gitgraph::Commit,
    _commit_a: &crate::model::gitgraph::Commit,
    lanes: &mut Vec<f64>,
    is_vert: bool,
    is_bt: bool,
) -> String {
    let p1x = pa.cx;
    let p1y = pa.cy;
    let p2x = pb.cx;
    let p2y = pb.cy;
    let radius = 10.0_f64;
    let offset = 10.0_f64;

    if is_vert {
        let line_x = if p1x < p2x {
            find_lane(p1x, p2x, lanes)
        } else {
            find_lane(p2x, p1x, lanes)
        };
        if is_bt {
            // BT (upstream lines 708-721): going up.
            if p1x < p2x {
                format!(
                    "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                    fmt_num(p1x), fmt_num(p1y),
                    fmt_num(line_x - radius), fmt_num(p1y),
                    fmt_num(radius), fmt_num(radius),
                    fmt_num(line_x), fmt_num(p1y - offset),
                    fmt_num(line_x), fmt_num(p2y + radius),
                    fmt_num(radius), fmt_num(radius),
                    fmt_num(line_x + offset), fmt_num(p2y),
                    fmt_num(p2x), fmt_num(p2y),
                )
            } else {
                format!(
                    "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                    fmt_num(p1x), fmt_num(p1y),
                    fmt_num(line_x + radius), fmt_num(p1y),
                    fmt_num(radius), fmt_num(radius),
                    fmt_num(line_x), fmt_num(p1y - offset),
                    fmt_num(line_x), fmt_num(p2y + radius),
                    fmt_num(radius), fmt_num(radius),
                    fmt_num(line_x - offset), fmt_num(p2y),
                    fmt_num(p2x), fmt_num(p2y),
                )
            }
        } else if p1x < p2x {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(line_x - radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(line_x),
                fmt_num(p1y + offset),
                fmt_num(line_x),
                fmt_num(p2y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(line_x + offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(line_x + radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(line_x),
                fmt_num(p1y + offset),
                fmt_num(line_x),
                fmt_num(p2y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(line_x - offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    } else {
        let line_y = if p1y < p2y {
            find_lane(p1y, p2y, lanes)
        } else {
            find_lane(p2y, p1y, lanes)
        };
        if p1y < p2y {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(line_y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(line_y),
                fmt_num(p2x - radius),
                fmt_num(line_y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(line_y + offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(line_y + radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(line_y),
                fmt_num(p2x - radius),
                fmt_num(line_y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(line_y - offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    }
}

/// Build the `d=` attribute for one parent → commit arrow (TB mode,
/// non-rerouted). Mirrors upstream `drawArrow` lines 747–777.
fn build_arrow_path_tb(
    pa: &crate::layout::gitgraph::CommitGeom,
    pb: &crate::layout::gitgraph::CommitGeom,
    commit_b: &crate::model::gitgraph::Commit,
    _commit_a: &crate::model::gitgraph::Commit,
) -> String {
    let p1x = pa.cx;
    let p1y = pa.cy;
    let p2x = pb.cx;
    let p2y = pb.cy;
    let radius = 20.0_f64;
    let offset = 20.0_f64;
    let is_merge_secondary = matches!(commit_b.kind, CommitKind::Merge)
        && commit_b.parents.first().map(|s| s.as_str()) != Some(_commit_a.id.as_str());

    if (p1x - p2x).abs() < f64::EPSILON {
        return format!(
            "M {} {} L {} {}",
            fmt_num(p1x),
            fmt_num(p1y),
            fmt_num(p2x),
            fmt_num(p2y)
        );
    }

    if p1x < p2x {
        // Source on left of dest. arc = A20 0 0 0, arc2 = A20 0 0 1.
        if is_merge_secondary {
            // M p1 L p1.x p2.y-r A20...0 p1.x+off p2.y L p2
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            // M p1 L p2.x-r p1.y A20...1 p2.x p1.y+off L p2
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x - radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y + offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    } else {
        // p1.x > p2.x: source on right of dest.
        if is_merge_secondary {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x - offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x + radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y + offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    }
}

/// Build the `d=` attribute for one parent → commit arrow (BT mode,
/// non-rerouted). Mirrors upstream `drawArrow` lines 778-809.
fn build_arrow_path_bt(
    pa: &crate::layout::gitgraph::CommitGeom,
    pb: &crate::layout::gitgraph::CommitGeom,
    commit_b: &crate::model::gitgraph::Commit,
    _commit_a: &crate::model::gitgraph::Commit,
) -> String {
    let p1x = pa.cx;
    let p1y = pa.cy;
    let p2x = pb.cx;
    let p2y = pb.cy;
    let radius = 20.0_f64;
    let offset = 20.0_f64;
    let is_merge_secondary = matches!(commit_b.kind, CommitKind::Merge)
        && commit_b.parents.first().map(|s| s.as_str()) != Some(_commit_a.id.as_str());

    if (p1x - p2x).abs() < f64::EPSILON {
        return format!(
            "M {} {} L {} {}",
            fmt_num(p1x),
            fmt_num(p1y),
            fmt_num(p2x),
            fmt_num(p2y)
        );
    }

    if p1x < p2x {
        if is_merge_secondary {
            // M p1 L p1.x p2.y+r A20...1 p1.x+off p2.y L p2
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y + radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            // M p1 L p2.x-r p1.y A20...0 p2.x p1.y-off L p2
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x - radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y - offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    } else {
        if is_merge_secondary {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y + radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x - offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x + radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y - offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    }
}

/// Build the `d=` attribute for one parent → commit arrow (LR mode).
///
/// Mirrors upstream `drawArrow` non-redux geometry. Only the
/// non-rerouted branch — rerouting is handled in
/// `build_arrow_path_rerouted`.
fn build_arrow_path(
    pa: &crate::layout::gitgraph::CommitGeom,
    pb: &crate::layout::gitgraph::CommitGeom,
    commit_b: &crate::model::gitgraph::Commit,
    _commit_a: &crate::model::gitgraph::Commit,
) -> String {
    let p1x = pa.cx;
    let p1y = pa.cy;
    let p2x = pb.cx;
    let p2y = pb.cy;
    let radius = 20.0_f64;
    let offset = 20.0_f64;
    let is_merge_secondary = matches!(commit_b.kind, CommitKind::Merge)
        && commit_b.parents.first().map(|s| s.as_str()) != Some(_commit_a.id.as_str());

    if (p1y - p2y).abs() < f64::EPSILON {
        return format!(
            "M {} {} L {} {}",
            fmt_num(p1x),
            fmt_num(p1y),
            fmt_num(p2x),
            fmt_num(p2y)
        );
    }

    if p1y < p2y {
        // Source above dest — descend then arc (clockwise CCW=0). Either:
        //   - merge with non-primary parent: `M p1 L p2.x-r p1.y A r r 0 0 1 p2.x p1.y+off L p2`
        //     (horizontal first then arc down-right)
        //   - normal: `M p1 L p1.x p2.y-r A r r 0 0 0 p1.x+off p2.y L p2`
        if is_merge_secondary {
            // p1.y < p2.y, secondary parent: horizontal, arc down.
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x - radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y + offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            // Normal downward: vertical, arc right.
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y - radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    } else {
        // p1.y > p2.y — source below dest (rising arrow).
        if is_merge_secondary {
            // Secondary parent rising: horizontal then arc up.
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 0, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p2x - radius),
                fmt_num(p1y),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p2x),
                fmt_num(p1y - offset),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        } else {
            // Normal upward: vertical then arc.
            format!(
                "M {} {} L {} {} A {} {}, 0, 0, 1, {} {} L {} {}",
                fmt_num(p1x),
                fmt_num(p1y),
                fmt_num(p1x),
                fmt_num(p2y + radius),
                fmt_num(radius),
                fmt_num(radius),
                fmt_num(p1x + offset),
                fmt_num(p2y),
                fmt_num(p2x),
                fmt_num(p2y),
            )
        }
    }
}

/// Format a number the way d3/jsdom does in mermaid output:
///   - integral values render without a decimal point ("0", "150").
///   - fractional values keep their full precision so the bytes match.
/// Render a branch name as one `<tspan>` per logical line. Mirrors
/// upstream `drawText`'s split on `\\n|\n|<br/>` — embedded `\n`
/// characters in the parsed name produce multi-row labels (cypress
/// fixture 105: `branch "Feature A\n(ongoing)"`). Each row is
/// `dy="1em" x="0"` so successive rows stack vertically with the same
/// left edge.
fn render_branch_tspans(name: &str) -> String {
    let mut out = String::new();
    for row in name.split('\n') {
        out.push_str(&format!(
            r#"<tspan xml:space="preserve" dy="1em" x="0" class="row">{}</tspan>"#,
            escape_text(row.trim())
        ));
    }
    out
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Strip whitespace after commas outside quoted segments, mirroring
/// stylis's CSS minification for the `font-family` value.
fn minify_font_family(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_quote = false;
    let mut prev_comma = false;
    for c in s.chars() {
        if c == '"' {
            in_quote = !in_quote;
            out.push(c);
            prev_comma = false;
            continue;
        }
        if !in_quote {
            if c == ',' {
                out.push(c);
                prev_comma = true;
                continue;
            }
            if prev_comma && c == ' ' {
                prev_comma = false;
                continue;
            }
        }
        out.push(c);
        prev_comma = false;
    }
    out
}

/// Compose the gitGraph CSS block — port of upstream
/// `diagrams/git/styles.js` with the default theme branch.
fn style_block(id: &str, theme: &ThemeVariables) -> String {
    let ff_raw = theme
        .font_family
        .as_deref()
        .unwrap_or("\"trebuchet ms\", verdana, arial, sans-serif");
    let ff = minify_font_family(ff_raw);
    let ff = ff.as_str();
    let fs = theme.font_size.as_deref().unwrap_or("16px");
    let text_color = theme.text_color.as_deref().unwrap_or("#333");
    let error_bkg = theme.error_bkg_color.as_deref().unwrap_or("#552222");
    let error_text = theme.error_text_color.as_deref().unwrap_or("#552222");
    let line_color = theme.line_color.as_deref().unwrap_or("#333333");
    let node_border = theme.node_border.as_deref().unwrap_or("#9370DB");
    let _main_bkg = theme.main_bkg.as_deref().unwrap_or("#ECECFF");
    let primary_color = theme.primary_color.as_deref().unwrap_or("#ECECFF");
    let stroke_width = theme.stroke_width.unwrap_or(1);
    let commit_label_bg = theme
        .commit_label_background
        .as_deref()
        .unwrap_or("#ffffde");
    let commit_label_color = theme.commit_label_color.as_deref().unwrap_or("#000021");
    let commit_label_size = theme.commit_label_font_size.as_deref().unwrap_or("10px");
    let tag_label_bg = theme.tag_label_background.as_deref().unwrap_or("#ECECFF");
    let tag_label_border = theme
        .tag_label_border
        .as_deref()
        .unwrap_or("hsl(240, 60%, 86.2745098039%)");
    let tag_label_color = theme.tag_label_color.as_deref().unwrap_or("#131300");
    let tag_label_size = theme.tag_label_font_size.as_deref().unwrap_or("10px");

    const GIT0: [&str; 8] = [
        "hsl(240, 100%, 46.2745098039%)",
        "hsl(60, 100%, 43.5294117647%)",
        "hsl(80, 100%, 46.2745098039%)",
        "hsl(210, 100%, 46.2745098039%)",
        "hsl(180, 100%, 46.2745098039%)",
        "hsl(150, 100%, 46.2745098039%)",
        "hsl(300, 100%, 46.2745098039%)",
        "hsl(0, 100%, 46.2745098039%)",
    ];
    const GIT_INV: [&str; 8] = [
        "hsl(60, 100%, 3.7254901961%)",
        "rgb(0, 0, 160.5)",
        "rgb(48.8333333334, 0, 146.5000000001)",
        "rgb(146.5000000001, 73.2500000001, 0)",
        "rgb(146.5000000001, 0, 0)",
        "rgb(146.5000000001, 0, 73.2500000001)",
        "rgb(0, 146.5000000001, 0)",
        "rgb(0, 146.5000000001, 146.5000000001)",
    ];
    const GIT_BRANCH_LABEL: [&str; 8] = [
        "#ffffff", "black", "black", "#ffffff", "black", "black", "black", "black",
    ];

    let mut css = String::with_capacity(8192);
    css.push_str(&format!(
        "<style>#{id}{{font-family:{ff};font-size:{fs};fill:{text_color};}}@keyframes edge-animation-frame{{from{{stroke-dashoffset:0;}}}}@keyframes dash{{to{{stroke-dashoffset:0;}}}}#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}#{id} .error-icon{{fill:{eb};}}#{id} .error-text{{fill:{et};stroke:{et};}}#{id} .edge-thickness-normal{{stroke-width:1px;}}#{id} .edge-thickness-thick{{stroke-width:3.5px;}}#{id} .edge-pattern-solid{{stroke-dasharray:0;}}#{id} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{id} .edge-pattern-dashed{{stroke-dasharray:3;}}#{id} .edge-pattern-dotted{{stroke-dasharray:2;}}#{id} .marker{{fill:{lc};stroke:{lc};}}#{id} .marker.cross{{stroke:{lc};}}#{id} svg{{font-family:{ff};font-size:{fs};}}#{id} p{{margin:0;}}#{id} .commit-id,#{id} .commit-msg,#{id} .branch-label{{fill:lightgrey;color:lightgrey;font-family:'trebuchet ms',verdana,arial,sans-serif;font-family:var(--mermaid-font-family);}}",
        eb = error_bkg, et = error_text, lc = line_color,
    ));

    for i in 0..12 {
        let ci = i % 8;
        css.push_str(&format!(
            "#{id} .branch-label{i}{{fill:{c};}}#{id} .commit{i}{{stroke:{g};fill:{g};}}#{id} .commit-highlight{i}{{stroke:{inv};fill:{inv};}}#{id} .label{i}{{fill:{g};}}#{id} .arrow{i}{{stroke:{g};}}",
            c = GIT_BRANCH_LABEL[ci],
            g = GIT0[ci],
            inv = GIT_INV[ci],
        ));
    }

    css.push_str(&format!(
        "#{id} .branch{{stroke-width:{sw};stroke:{lc};stroke-dasharray:2;}}#{id} .commit-label{{font-size:{cls};fill:{clc};}}#{id} .commit-label-bkg{{font-size:{cls};fill:{clb};opacity:0.5;}}#{id} .tag-label{{font-size:{tls};fill:{tlc};}}#{id} .tag-label-bkg{{fill:{tlb};stroke:{tlbo};}}#{id} .tag-hole{{fill:{tc};}}#{id} .commit-merge{{stroke:{pc};fill:{pc};}}#{id} .commit-reverse{{stroke:{pc};fill:{pc};stroke-width:3;}}#{id} .commit-highlight-inner{{stroke:{pc};fill:{pc};}}#{id} .arrow{{stroke-width:8;stroke-linecap:round;fill:none;}}#{id} .gitTitleText{{text-anchor:middle;font-size:18px;fill:{tc};}}",
        sw = stroke_width, lc = line_color,
        cls = commit_label_size, clc = commit_label_color, clb = commit_label_bg,
        tls = tag_label_size, tlc = tag_label_color, tlb = tag_label_bg, tlbo = tag_label_border,
        tc = text_color, pc = primary_color,
    ));

    css.push_str(&format!(
        "#{id} .node .neo-node{{stroke:{nb};}}#{id} [data-look=\"neo\"].node rect,#{id} [data-look=\"neo\"].cluster rect,#{id} [data-look=\"neo\"].node polygon{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node path{{stroke:{nb};stroke-width:1px;}}#{id} [data-look=\"neo\"].node .outer-path{{filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node .neo-line path{{stroke:{nb};filter:none;}}#{id} [data-look=\"neo\"].node circle{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].node circle .state-start{{fill:{mb};}}#{id} [data-look=\"neo\"].icon-shape .icon{{fill:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} [data-look=\"neo\"].icon-shape .icon-neo path{{stroke:{nb};filter:drop-shadow(1px 2px 2px rgba(185, 185, 185, 1));}}#{id} :root{{--mermaid-font-family:{ff};}}",
        nb = node_border, mb = "#000000", ff = ff,
    ));
    css.push_str("</style>");
    css
}
