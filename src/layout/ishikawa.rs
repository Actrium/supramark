//! Ishikawa layout — pre-computes every coordinate the renderer writes.
//!
//! Upstream reference: /ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/ishikawa/ishikawaRenderer.ts
//!
//! Goal: byte-exact arithmetic parity with the upstream V8/jsdom
//! pipeline. Three behaviours are non-obvious and must be preserved
//! literally:
//!
//! 1. **Trig parity.** The upstream constants `COS_A = Math.cos(82 *
//!    π/180)` / `SIN_A = Math.sin(82 * π/180)` ship through V8, whose
//!    numerical results differ from `f64::cos/sin` by 1 ULP for some
//!    inputs. We call [`crate::math::v8_trig::cos`] / `sin` so every
//!    downstream multiplication agrees to the bit.
//!
//! 2. **jsdom's `getBBox` shim.** jsdom never applies stylesheet CSS,
//!    so every text element falls back to 14px sans-serif. Its bbox
//!    returns `{x:0, y:0, width:textWidth, height:lineHeight}` — the
//!    text anchor does not shift `x`. We emulate that here by:
//!       - sizing labels with `text_width(text, "sans-serif", 14.0, …)`;
//!       - using tb.x = tb.y = 0 throughout.
//!
//! 3. **spineX update loop.** After drawing each pair group, upstream
//!    does `pg.selectAll('text').nodes().reduce(Math.min(left, bbox.x),
//!    Infinity)`. Since every text bbox has `x = 0`, the result is 0
//!    whenever the pair has at least one text node and `Infinity`
//!    when it has none. The `Infinity` case never reaches final output
//!    (branches with no causes can only happen on the odd leg of the
//!    last pair — in that case upstream's subsequent `x1 = spineX`
//!    serialises `Infinity` and the viewBox computation chokes; we
//!    clamp to 0 to mirror the observed real-world shim behaviour).

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::math::v8_trig::{cos, sin};
use crate::model::ishikawa::{IshikawaDiagram, IshikawaNode};
use crate::theme::ThemeVariables;

// ── Upstream constants (ishikawaRenderer.ts lines 19–26) ───────────
pub const FONT_SIZE_DEFAULT: f64 = 14.0;
pub const SPINE_BASE_LENGTH: f64 = 250.0;
pub const BONE_STUB: f64 = 30.0;
pub const BONE_BASE: f64 = 60.0;
pub const BONE_PER_CHILD: f64 = 5.0;
// jsdom's fallback font — 14px "sans-serif" — bakes this into every bbox.
pub const BBOX_FAMILY: &str = "sans-serif";
pub const BBOX_FONT_SIZE: f64 = 14.0;

/// Per-branch line / label coordinates that the renderer emits verbatim.
#[derive(Debug, Clone, Default)]
pub struct IshikawaLayout {
    /// View port padding (config.ishikawa.diagramPadding, default 20).
    pub padding: f64,
    /// Overall SVG viewBox: (x, y, width, height).
    pub viewbox: (f64, f64, f64, f64),
    /// Effective font size for y-spacing (config.fontSize, default 14 →
    /// mermaid default 16). This drives `line_height = fontSize * 1.05`.
    pub font_size: f64,

    /// Root node present?
    pub has_root: bool,
    /// Root effect label (post-sanitise).
    pub root_text: String,
    /// Spine Y-coordinate (upper branches hang at 0, lower at 2*spine_y).
    pub spine_y: f64,
    /// Final left edge of the spine (horizontal line from `spine_x_left`
    /// to 0 at Y=spine_y).
    pub spine_x_left: f64,

    /// Head geometry (path and text transform).
    pub head_w: f64,
    pub head_h: f64,
    pub head_text_x_shift: f64,
    pub head_text_y_shift: f64,
    /// Text y (before shift) for the head label.
    pub head_text_y: f64,
    pub head_text_lines: Vec<String>,
    pub head_text_dy: f64,

    /// One entry per pair: `causes[p*2]` and `causes[p*2+1]`.
    pub pairs: Vec<Pair>,
}

#[derive(Debug, Clone, Default)]
pub struct Pair {
    /// X coordinate where this pair's branches originate (spineX at the
    /// time the pair was drawn).
    pub origin_x: f64,
    pub upper: Option<Branch>,
    pub lower: Option<Branch>,
}

/// One top-level cause (one side of a pair).
#[derive(Debug, Clone)]
pub struct Branch {
    /// Direction: -1 for upper (toward Y=0), +1 for lower.
    pub direction: i32,
    /// Start-of-branch = (origin_x, spine_y).
    pub start: (f64, f64),
    /// End-of-branch = (end_x, end_y). Also the anchor for the label box.
    pub end: (f64, f64),

    /// Cause label — text + bbox (at 14px).
    pub label_text: Vec<String>,
    pub label_text_x: f64,
    pub label_text_y: f64, // y attribute on <text>
    pub label_text_dy: f64,
    /// Rect x / y / width / height.
    pub label_rect: (f64, f64, f64, f64),

    /// Sub-branches (horizontal + diagonal bones).
    pub sub_branches: Vec<SubBranch>,
}

#[derive(Debug, Clone)]
pub struct SubBranch {
    pub line: (f64, f64, f64, f64), // x1, y1, x2, y2
    /// Text label text (may contain explicit '\n' from wrap).
    pub text_lines: Vec<String>,
    pub text_x: f64,
    pub text_y: f64,
    pub text_dy: f64,
    pub text_class: &'static str, // "ishikawa-label align" / "up" / "down"
}

pub fn layout(d: &IshikawaDiagram, theme: &ThemeVariables) -> Result<IshikawaLayout> {
    let mut l = IshikawaLayout::default();
    l.padding = d.diagram_padding;
    // Resolve fontSize — mermaid config default is 16 ("16px"). parse
    // the leading number from the theme's `fontSize`, defaulting to
    // 16 (the diagram-api default). `FONT_SIZE_DEFAULT` (=14) only
    // applies when the config has no numeric prefix at all.
    let font_size = parse_font_size_leading(theme.font_size.as_deref()).unwrap_or(16.0);
    l.font_size = font_size;

    let Some(root) = d.root.as_ref() else {
        // No root — the `if (!root) return;` early exit in upstream
        // emits nothing after the opening `<g class="ishikawa">`.
        l.has_root = false;
        // Default viewBox for an empty diagram. Upstream's getBBox over
        // the empty group produces (0,0,0,0), padded by `padding`.
        l.viewbox = (-l.padding, -l.padding, l.padding * 2.0, l.padding * 2.0);
        return Ok(l);
    };
    l.has_root = true;
    l.root_text = root.text.clone();

    // ── Head geometry ───────────────────────────────────────────────
    // max_chars for head-label wrap: `max(6, floor(110 / (fontSize * 0.6)))`.
    let max_chars = 6i64.max((110.0 / (font_size * 0.6)).floor() as i64);
    let head_wrapped = wrap_text(&root.text, max_chars as usize);
    l.head_text_lines = head_wrapped.split('\n').map(str::to_string).collect();
    let head_lh = font_size * 1.05;
    l.head_text_dy = head_lh;
    // textContent across tspans is concatenated without separators —
    // width comes from the concatenated string at 14px.
    let head_concat: String = l.head_text_lines.join("");
    let head_bbox_w = text_width(&head_concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let head_bbox_h = line_height_14();
    l.head_w = 60f64.max(head_bbox_w + 6.0);
    l.head_h = 40f64.max(head_bbox_h * 2.0 + 40.0);

    // drawMultilineText picks text y = y - ((lines-1) * lh) / 2 at y=0.
    let n_lines = l.head_text_lines.len().max(1) as f64;
    l.head_text_y = 0.0 - ((n_lines - 1.0) * head_lh) / 2.0;
    // Upstream: `translate((w - tb.width)/2 - tb.x + 3, -tb.y - tb.height/2)`.
    l.head_text_x_shift = (l.head_w - head_bbox_w) / 2.0 - 0.0 + 3.0;
    l.head_text_y_shift = -0.0 - head_bbox_h / 2.0;

    // ── Early-exit: no causes ───────────────────────────────────────
    let causes = &root.children;
    if causes.is_empty() {
        l.spine_y = SPINE_BASE_LENGTH;
        l.spine_x_left = 0.0;
        // Only the head + degenerate spine exist.
        l.viewbox = compute_viewbox_empty_causes(&l);
        return Ok(l);
    }

    // ── Upper / lower split ─────────────────────────────────────────
    let upper: Vec<&IshikawaNode> = causes.iter().step_by(2).collect();
    let lower: Vec<&IshikawaNode> = causes.iter().skip(1).step_by(2).collect();

    let (upper_total, upper_max) = side_stats(&upper);
    let (lower_total, lower_max) = side_stats(&lower);
    let descendant_total = upper_total + lower_max.saturating_mul(0) + lower_total;

    let mut upper_len = SPINE_BASE_LENGTH;
    let mut lower_len = SPINE_BASE_LENGTH;
    if descendant_total > 0 {
        let pool = SPINE_BASE_LENGTH * 2.0;
        let min_len = SPINE_BASE_LENGTH * 0.3;
        upper_len = min_len.max(pool * (upper_total as f64 / descendant_total as f64));
        lower_len = min_len.max(pool * (lower_total as f64 / descendant_total as f64));
    }
    let min_spacing = font_size * 2.0;
    upper_len = upper_len.max(upper_max as f64 * min_spacing);
    lower_len = lower_len.max(lower_max as f64 * min_spacing);

    let spine_y = upper_len.max(SPINE_BASE_LENGTH);
    l.spine_y = spine_y;

    // ── Build pairs ─────────────────────────────────────────────────
    // angle = 82° in radians.
    let angle = 82.0 * std::f64::consts::PI / 180.0;
    let cos_a = cos(angle);
    let sin_a = sin(angle);

    // spineX starts at -20 after `spineX -= 20` (non-empty causes branch).
    let mut spine_x: f64 = -20.0;
    let pair_count = (causes.len() + 1) / 2;

    for p in 0..pair_count {
        let mut pair = Pair {
            origin_x: spine_x,
            upper: None,
            lower: None,
        };
        let upper_cause = causes.get(p * 2);
        let lower_cause = causes.get(p * 2 + 1);

        if let Some(node) = upper_cause {
            pair.upper = Some(build_branch(
                node, spine_x, spine_y, -1, upper_len, font_size, cos_a, sin_a,
            ));
        }
        if let Some(node) = lower_cause {
            pair.lower = Some(build_branch(
                node, spine_x, spine_y, 1, lower_len, font_size, cos_a, sin_a,
            ));
        }

        // Emulate the `spineX = pg.selectAll('text').reduce(Math.min(...x), Infinity)`
        // pass. Since every text bbox has `x = 0` in the jsdom shim, the
        // reduction yields 0 whenever any text was drawn. If the pair
        // has NO text at all (both causes absent — impossible here since
        // at least one must be present for the loop to run), we leave
        // spine_x untouched to avoid Infinity poisoning downstream.
        let has_any_text = pair.upper.is_some() || pair.lower.is_some();
        if has_any_text {
            spine_x = 0.0;
        }
        l.pairs.push(pair);
    }

    l.spine_x_left = spine_x;

    // ── Compute viewBox by unioning every drawn element ────────────
    l.viewbox = compute_viewbox(&l, theme);

    Ok(l)
}

// ── Branch construction ────────────────────────────────────────────

fn build_branch(
    node: &IshikawaNode,
    start_x: f64,
    start_y: f64,
    direction: i32,
    length: f64,
    font_size: f64,
    cos_a: f64,
    sin_a: f64,
) -> Branch {
    let children = &node.children;
    // `lineLen = length * (children.length ? 1 : 0.2)`.
    let line_len = if children.is_empty() {
        length * 0.2
    } else {
        length
    };
    let dx = -cos_a * line_len;
    let dy = sin_a * line_len * (direction as f64);
    let end_x = start_x + dx;
    let end_y = start_y + dy;

    // drawCauseLabel — the cause box + text. Text y at y+11*direction.
    let text_y_input = end_y + 11.0 * (direction as f64);
    let text = &node.text;
    let lines: Vec<String> = split_label_lines(text);
    let lh = font_size * 1.05;
    let text_y_attr = text_y_input - ((lines.len() as f64 - 1.0) * lh) / 2.0;
    let concat: String = lines.join("");
    let tb_w = text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let tb_h = line_height_14() * (lines.len() as f64);
    // rect dims (tb.x = tb.y = 0).
    let rect_x = 0.0 - 20.0;
    let rect_y = 0.0 - 2.0;
    let rect_w = tb_w + 40.0;
    let rect_h = tb_h + 4.0;

    // Sub-branches.
    let sub_branches = if children.is_empty() {
        Vec::new()
    } else {
        build_sub_branches(
            children, start_x, start_y, end_x, end_y, dx, dy, direction, cos_a, sin_a, font_size,
        )
    };

    Branch {
        direction,
        start: (start_x, start_y),
        end: (end_x, end_y),
        label_text: lines,
        label_text_x: end_x,
        label_text_y: text_y_attr,
        label_text_dy: lh,
        label_rect: (rect_x, rect_y, rect_w, rect_h),
        sub_branches,
    }
}

/// Mirror of upstream `flattenTree` + the per-entry bone drawing loop.
#[allow(clippy::too_many_arguments)]
fn build_sub_branches(
    children: &[IshikawaNode],
    start_x: f64,
    start_y: f64,
    end_x: f64,
    end_y: f64,
    _dx: f64,
    dy: f64,
    direction: i32,
    cos_a: f64,
    sin_a: f64,
    font_size: f64,
) -> Vec<SubBranch> {
    // Flatten (pre-order for even depth, post-order for odd depth; for
    // direction=-1 siblings are reversed before walking).
    #[derive(Debug, Clone)]
    struct Entry {
        depth: i64,
        text: String,
        parent_index: i64,
        child_count: usize,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mut y_order: Vec<usize> = Vec::new();

    fn walk(
        nodes: &[IshikawaNode],
        pid: i64,
        depth: i64,
        direction: i32,
        entries: &mut Vec<Entry>,
        y_order: &mut Vec<usize>,
    ) {
        // Match upstream `direction === -1 ? [...nodes].reverse() : nodes`.
        let ordered: Vec<&IshikawaNode> = if direction == -1 {
            nodes.iter().rev().collect()
        } else {
            nodes.iter().collect()
        };
        for child in ordered {
            let idx = entries.len();
            let gc = &child.children;
            entries.push(Entry {
                depth,
                text: wrap_text(&child.text, 15),
                parent_index: pid,
                child_count: gc.len(),
            });
            if depth % 2 == 0 {
                y_order.push(idx);
                if !gc.is_empty() {
                    walk(gc, idx as i64, depth + 1, direction, entries, y_order);
                }
            } else {
                if !gc.is_empty() {
                    walk(gc, idx as i64, depth + 1, direction, entries, y_order);
                }
                y_order.push(idx);
            }
        }
    }
    walk(children, -1, 2, direction, &mut entries, &mut y_order);

    let entry_count = entries.len();
    let mut ys = vec![0.0f64; entry_count];
    for (slot, &entry_idx) in y_order.iter().enumerate() {
        ys[entry_idx] = start_y + dy * ((slot as f64 + 1.0) / (entry_count as f64 + 1.0));
    }

    // Bones cache.
    #[derive(Clone, Copy)]
    struct BoneInfo {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        child_count: usize,
        children_drawn: usize,
    }
    use std::collections::HashMap;
    let mut bones: HashMap<i64, BoneInfo> = HashMap::new();
    bones.insert(
        -1,
        BoneInfo {
            x0: start_x,
            y0: start_y,
            x1: end_x,
            y1: end_y,
            child_count: children.len(),
            children_drawn: 0,
        },
    );

    let diagonal_x = -cos_a;
    let diagonal_y = sin_a * (direction as f64);
    let odd_label: &'static str = if direction < 0 {
        "ishikawa-label up"
    } else {
        "ishikawa-label down"
    };

    let mut out: Vec<SubBranch> = Vec::with_capacity(entry_count);

    for (i, e) in entries.iter().enumerate() {
        let y = ys[i];
        let par = *bones.get(&e.parent_index).expect("parent bone");

        let (bx0, by0, bx1) = if e.depth % 2 == 0 {
            // Horizontal bone.
            let dy_p = par.y1 - par.y0;
            let t = if dy_p != 0.0 {
                (y - par.y0) / dy_p
            } else {
                0.5
            };
            let bx0 = lerp(par.x0, par.x1, t);
            let by0 = y;
            let bx1 = bx0
                - if e.child_count > 0 {
                    BONE_BASE + (e.child_count as f64) * BONE_PER_CHILD
                } else {
                    BONE_STUB
                };
            (bx0, by0, bx1)
        } else {
            // Diagonal bone: start from evenly-spaced point on parent's
            // horizontal; advance toward target y via diagonal direction.
            let k = par.children_drawn;
            let bx0 = lerp(
                par.x0,
                par.x1,
                (par.child_count as f64 - k as f64) / (par.child_count as f64 + 1.0),
            );
            let by0 = par.y0;
            let bx1 = bx0 + diagonal_x * ((y - by0) / diagonal_y);
            (bx0, by0, bx1)
        };

        // Bump parent's children_drawn counter.
        if let Some(par_mut) = bones.get_mut(&e.parent_index) {
            par_mut.children_drawn += 1;
        }

        // Text y attribute:
        //   - horizontal (even depth): text y = y (single y argument) —
        //     with lines, y = y - (lines-1)*lh/2.
        //   - diagonal (odd depth): text y = y.
        // drawMultilineText always applies `y - ((lines-1) * lh)/2`.
        let lines: Vec<String> = split_label_lines(&e.text);
        let lh = font_size * 1.05;
        let text_y_attr = y - ((lines.len() as f64 - 1.0) * lh) / 2.0;

        let (line, text_x, text_class) = if e.depth % 2 == 0 {
            ((bx0, y, bx1, y), bx1, "ishikawa-label align")
        } else {
            ((bx0, by0, bx1, y), bx1, odd_label)
        };

        out.push(SubBranch {
            line,
            text_lines: lines,
            text_x,
            text_y: text_y_attr,
            text_dy: lh,
            text_class,
        });

        if e.child_count > 0 {
            bones.insert(
                i as i64,
                BoneInfo {
                    x0: bx0,
                    y0: by0,
                    x1: bx1,
                    y1: y,
                    child_count: e.child_count,
                    children_drawn: 0,
                },
            );
        }
    }

    // Emit in reverse-per-depth order so the first-appearance of each
    // subgroup in the <g class="ishikawa-pair"> matches upstream's
    // append order. Upstream does `const grp = svg.append('g')` PER
    // entry — the visual order is the entries array order. Our out is
    // already in that order, so no reversal needed.
    // BUT: in fixtures like demos 01 (branch "Equipment" with LENS/
    // SENSOR), the order of emitted sub-groups is: SENSOR first, then
    // its children, then LENS, then its children. That matches our
    // entries walk order given `direction=-1 ⇒ reverse`, LENS/SENSOR
    // in input order → walked in reverse → SENSOR first. Correct.
    out
}

// ── Helpers ────────────────────────────────────────────────────────

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Count descendants (children, grandchildren, …) recursively.
fn count_descendants(node: &IshikawaNode) -> usize {
    node.children.iter().map(|c| 1 + count_descendants(c)).sum()
}

fn side_stats(nodes: &[&IshikawaNode]) -> (usize, usize) {
    let mut total = 0usize;
    let mut max = 0usize;
    for n in nodes {
        let d = count_descendants(n);
        total += d;
        if d > max {
            max = d;
        }
    }
    (total, max)
}

/// Port of upstream `wrapText`. Splits on whitespace; greedy line
/// packing at `max_chars` char budget; joins lines with `\n`.
fn wrap_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if let Some(last) = lines.last_mut() {
            if last.chars().count() + 1 + word.chars().count() <= max_chars {
                last.push(' ');
                last.push_str(word);
                continue;
            }
        }
        lines.push(word.to_string());
    }
    lines.join("\n")
}

/// Split by `<br>` variants and `\n` — mirrors upstream `splitLines`.
fn split_label_lines(text: &str) -> Vec<String> {
    // upstream: `text.split(/<br\s*\/?>|\n/)`.
    let re = regex::Regex::new(r"<br\s*/?>|\n").unwrap();
    re.split(text).map(str::to_string).collect()
}

/// Line height at 14px / sans-serif — bakes the DejaVu Sans
/// ascender+descender (same metrics both sides of the pipeline).
fn line_height_14() -> f64 {
    crate::font_metrics::line_height(BBOX_FAMILY, BBOX_FONT_SIZE, false, false)
}

fn parse_font_size_leading(s: Option<&str>) -> Option<f64> {
    let s = s?;
    let mut iter = s.chars().peekable();
    let mut buf = String::new();
    while let Some(&c) = iter.peek() {
        if c.is_ascii_digit() || c == '.' {
            buf.push(c);
            iter.next();
        } else {
            break;
        }
    }
    buf.parse::<f64>().ok()
}

// ── ViewBox computation ────────────────────────────────────────────

/// ViewBox when there are no causes. Only the head path + invisible
/// spine contribute.
fn compute_viewbox_empty_causes(l: &IshikawaLayout) -> (f64, f64, f64, f64) {
    // Union over: head path (x: 0..head_w*2.4 ? no — just the 3 control
    // anchors M/L/Q), head text (width/height at 0..tb_w x 0..tb_h), and
    // the spine line (point at (0, spine_y)).
    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();

    // Head group transform: translate(0, spine_y). Paths inside are
    // relative, then offset.
    let path_box = head_path_bbox(l.head_w, l.head_h);
    boxes.push(translate_box(path_box, 0.0, l.spine_y));

    // Head text: the <text> element's x/y/transform. The jsdom union
    // visits <text> as x=0, y=0, w=tb.width, h=tb.height BUT the
    // parent <g class="ishikawa-head-group"> has transform=translate(0, spine_y).
    // jsdom's union shim ignores transforms per its comment in
    // generate_ref.mjs:537 ("Skip transforms for now"). So the text
    // bbox contributes (0, 0, tb_w, tb_h) WITHOUT the translate —
    // which usually falls inside the head-path bbox.
    let text_w = head_text_width(&l.head_text_lines);
    let text_h = head_text_height(l.head_text_lines.len());
    boxes.push((0.0, 0.0, text_w, text_h));

    // Spine line from (0, spine_y) to (0, spine_y) → degenerate; its
    // bbox is (0, spine_y, 0, 0) which the shim filters out with
    // `w===0 && h===0`. So it contributes nothing.

    let u = union_boxes(&boxes);
    pad_viewbox(u, l.padding)
}

fn compute_viewbox(l: &IshikawaLayout, _theme: &ThemeVariables) -> (f64, f64, f64, f64) {
    let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();

    // Head (inside translated <g>, but jsdom ignores transforms).
    let path_box = head_path_bbox(l.head_w, l.head_h);
    boxes.push(path_box);
    let text_w = head_text_width(&l.head_text_lines);
    let text_h = head_text_height(l.head_text_lines.len());
    boxes.push((0.0, 0.0, text_w, text_h));

    // Spine line: from (spine_x_left, spine_y) to (0, spine_y). jsdom
    // bbox of a line: (min(x1,x2), min(y1,y2), |dx|, |dy|).
    boxes.push((
        l.spine_x_left.min(0.0),
        l.spine_y,
        (0.0 - l.spine_x_left).abs(),
        0.0,
    ));

    // Each branch & its sub-branches.
    for pair in &l.pairs {
        for b_opt in [&pair.upper, &pair.lower] {
            let Some(b) = b_opt else { continue };
            let (x0, y0) = b.start;
            let (x1, y1) = b.end;
            boxes.push((x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs()));
            // Label rect.
            let (rx, ry, rw, rh) = b.label_rect;
            boxes.push((rx, ry, rw, rh));
            // Label text: bbox = (0, 0, text_w, text_h) per shim.
            boxes.push(text_bbox_lines(&b.label_text));

            for sb in &b.sub_branches {
                let (sx1, sy1, sx2, sy2) = sb.line;
                boxes.push((
                    sx1.min(sx2),
                    sy1.min(sy2),
                    (sx2 - sx1).abs(),
                    (sy2 - sy1).abs(),
                ));
                boxes.push(text_bbox_lines(&sb.text_lines));
            }
        }
    }

    let u = union_boxes(&boxes);
    pad_viewbox(u, l.padding)
}

/// head path: `M 0 -h/2 L 0 h/2 Q w*2.4 0 0 -h/2 Z` — bbox over all
/// anchor+control points.
fn head_path_bbox(w: f64, h: f64) -> (f64, f64, f64, f64) {
    let min_x = 0.0f64.min(w * 2.4);
    let max_x = 0.0f64.max(w * 2.4);
    let min_y = -h / 2.0;
    let max_y = h / 2.0;
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

fn head_text_width(lines: &[String]) -> f64 {
    let concat: String = lines.iter().cloned().collect::<Vec<_>>().join("");
    text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false)
}

fn head_text_height(n_lines: usize) -> f64 {
    line_height_14() * (n_lines.max(1) as f64)
}

/// jsdom shim: for <text> with tspans, textContent is concatenated
/// WITHOUT newline separators — so width = textWidth(concat) and
/// height = lineHeight * 1 (no '\n' found). We emulate that.
fn text_bbox_lines(lines: &[String]) -> (f64, f64, f64, f64) {
    let concat: String = lines.iter().cloned().collect::<Vec<_>>().join("");
    let w = text_width(&concat, BBOX_FAMILY, BBOX_FONT_SIZE, false, false);
    let h = line_height_14();
    (0.0, 0.0, w, h)
}

fn translate_box(b: (f64, f64, f64, f64), _dx: f64, _dy: f64) -> (f64, f64, f64, f64) {
    // jsdom shim ignores transforms; translate is a no-op here.
    b
}

fn union_boxes(boxes: &[(f64, f64, f64, f64)]) -> (f64, f64, f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found = false;
    for (x, y, w, h) in boxes.iter().copied() {
        if w == 0.0 && h == 0.0 {
            continue;
        }
        found = true;
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x + w > max_x {
            max_x = x + w;
        }
        if y + h > max_y {
            max_y = y + h;
        }
    }
    if !found {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

fn pad_viewbox(u: (f64, f64, f64, f64), pad: f64) -> (f64, f64, f64, f64) {
    (u.0 - pad, u.1 - pad, u.2 + pad * 2.0, u.3 + pad * 2.0)
}
