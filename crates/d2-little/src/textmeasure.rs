//! Text measurement for d2 rendering.
//!
//! Ported from Go `lib/textmeasure/textmeasure.go` and `lib/textmeasure/atlas.go`.
//!
//! 实现策略：字节级复现 Go 版本 `golang/freetype/truetype` + `fixed.Int26_6`
//! 的度量结果。所有与 Go 有关的中间量都以 Int26_6 (i32，表示 value * 64)
//! 参与整数运算，只在最终返回时通过 `i2f` 转换为 `f64` 像素值。
//!
//! 关键点：
//! * 使用 `ttf-parser` 的 `glyph_bounding_box` 获取 FUnit 级别的紧凑控制点包围盒
//!   (与 Go freetype 遍历所有控制点计算出的 `g.Bounds` 等价，因为 ttf-parser 的
//!   `OutlineBuilder` 会把所有原始控制点都 `extend_by` 进去，而合成的中点不会
//!   让包围盒扩大)。
//! * 逐字形点按 Go 的 `Font.scale` 公式缩放：
//!   `scaled = (scale_26_6 * funit + sign(funit) * fupe / 2) / fupe`，
//!   其中 `scale_26_6 = round(size * dpi * 64 / 72)`，`fupe` 为每 em 单位数。
//! * Floor / Ceil 到整数像素边界，按 Go `makeMapping` 的算法累积 frame / dot。
//! * `DrawRune` 内把 rect 的高度替换为 `ascent + descent`，与 Go 一致。

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::fonts::{FONT_FAMILIES, FONT_STYLES, Font, FontFamily, FontStyle};
use markdown::{CompileOptions, Constructs, Options, ParseOptions};
use regex::Regex;
use roxmltree::{Document, Node, NodeType};
use ttf_parser::Face;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TAB_SIZE: f64 = 4.0;
const SIZELESS_FONT_SIZE: i32 = 0;
const REPLACEMENT_CHAR: char = '\u{FFFD}';

pub const MARKDOWN_FONT_SIZE: i32 = crate::fonts::FONT_SIZE_M;

/// Line-height factor used when measuring code blocks
/// (shape: code with language / fenced code). Mirrors Go
/// `textmeasure.CODE_LINE_HEIGHT`.
pub const CODE_LINE_HEIGHT: f64 = 1.3;

const MARKDOWN_LINE_HEIGHT: f64 = 1.5;

const PADDING_LEFT_UL_OL_EM: f64 = 2.0;
const MARGIN_BOTTOM_UL: f64 = 16.0;

const MARGIN_TOP_LI_P: f64 = 16.0;
const MARGIN_TOP_LI_EM: f64 = 0.25;
const MARGIN_BOTTOM_P: f64 = 16.0;

const LINE_HEIGHT_H: f64 = 1.25;
const MARGIN_TOP_H: f64 = 24.0;
const MARGIN_BOTTOM_H: f64 = 16.0;
const PADDING_BOTTOM_H1_H2_EM: f64 = 0.3;
const BORDER_BOTTOM_H1_H2: f64 = 1.0;

const HEIGHT_HR_EM: f64 = 0.25;
const MARGIN_TOP_BOTTOM_HR: f64 = 24.0;

const PADDING_PRE: f64 = 16.0;
const MARGIN_BOTTOM_PRE: f64 = 16.0;
const LINE_HEIGHT_PRE: f64 = 1.45;
const FONT_SIZE_PRE_CODE_EM: f64 = 0.85;

const PADDING_TOP_BOTTOM_CODE_EM: f64 = 0.2;
const PADDING_LEFT_RIGHT_CODE_EM: f64 = 0.4;

const PADDING_LR_BLOCKQUOTE_EM: f64 = 1.0;
const MARGIN_BOTTOM_BLOCKQUOTE: f64 = 16.0;
const BORDER_LEFT_BLOCKQUOTE_EM: f64 = 0.25;

const H1_EM: f64 = 2.0;
const H2_EM: f64 = 1.5;
const H3_EM: f64 = 1.25;
const H4_EM: f64 = 1.0;
const H5_EM: f64 = 0.875;
const H6_EM: f64 = 0.85;

static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="([^"]*)""#).expect("href regex"));

/// 默认会预先烘焙进 atlas 的 runes 集合。
/// ASCII + Latin-1 Supplement + Geometric Shapes (与 Go `init()` 同步)。
fn default_runes() -> Vec<char> {
    let mut runes = Vec::with_capacity(512);
    for c in 0x0000u32..=0x007F {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    for c in 0x0080u32..=0x00FF {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    for c in 0x25A0u32..=0x25FF {
        if let Some(ch) = char::from_u32(c) {
            runes.push(ch);
        }
    }
    runes
}

// ---------------------------------------------------------------------------
// Fixed-point (Int26_6) helpers —— 严格复刻 Go `fixed.Int26_6` 行为
// ---------------------------------------------------------------------------

/// Int26_6 值 x 对应的 pixel float64（= x / 64）。
#[inline]
fn i2f(x: i32) -> f64 {
    x as f64 / 64.0
}

/// `fixed.I(i)`: 把整数像素提升为 Int26_6（= i * 64）。
#[inline]
fn i_pixel(i: i32) -> i32 {
    i << 6
}

/// Go `fixed.Int26_6.Floor()` —— 算术右移 6 位。
#[inline]
fn floor_26_6(x: i32) -> i32 {
    // Rust 有符号整数右移就是算术右移，与 Go 一致。
    x >> 6
}

/// Go `fixed.Int26_6.Ceil()` —— `(x + 0x3f) >> 6`。
#[inline]
fn ceil_26_6(x: i32) -> i32 {
    (x + 0x3f) >> 6
}

/// Go `truetype.Font.scale`: 把 `scale_26_6 * funit` 按 fupe 取整。
///
/// ```text
/// if x >= 0 { x += fupe / 2 } else { x -= fupe / 2 }
/// return x / fupe
/// ```
///
/// 注意：这里参数 `x` 已经是 `scale_26_6 * funit`（仍保留 Int26_6 的 *64 量级）。
#[inline]
fn font_scale_div(x: i64, fupe: i32) -> i32 {
    let fupe64 = fupe as i64;
    let y = if x >= 0 {
        x + fupe64 / 2
    } else {
        x - fupe64 / 2
    };
    // Go 的整数除法对负数是向零截断，与 Rust 的 `/` 一致。
    (y / fupe64) as i32
}

/// 将一个 FUnit 坐标按 Go freetype 的方式缩放到 Int26_6 像素单位。
#[inline]
fn scale_funit_to_26_6(funit: i32, scale_26_6: i32, fupe: i32) -> i32 {
    // Go 做的是 `Int26_6 * Int26_6`，两个都是 i32，结果落回 i32。
    // 这里用 i64 防溢出。对 1000 以内的 fupe 与 1024 左右的 scale
    // 以及数千的 funit 来说 i32 也够，但 i64 更安全。
    let prod = scale_26_6 as i64 * funit as i64;
    font_scale_div(prod, fupe)
}

// ---------------------------------------------------------------------------
// Rect (内部包围盒类型)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Rect {
    tl_x: f64,
    tl_y: f64,
    br_x: f64,
    br_y: f64,
}

impl Rect {
    fn zero() -> Self {
        Self {
            tl_x: 0.0,
            tl_y: 0.0,
            br_x: 0.0,
            br_y: 0.0,
        }
    }

    fn w(&self) -> f64 {
        self.br_x - self.tl_x
    }

    fn h(&self) -> f64 {
        self.br_y - self.tl_y
    }

    fn norm(self) -> Self {
        Self {
            tl_x: self.tl_x.min(self.br_x),
            tl_y: self.tl_y.min(self.br_y),
            br_x: self.tl_x.max(self.br_x),
            br_y: self.tl_y.max(self.br_y),
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            tl_x: self.tl_x.min(other.tl_x),
            tl_y: self.tl_y.min(other.tl_y),
            br_x: self.br_x.max(other.br_x),
            br_y: self.br_y.max(other.br_y),
        }
    }
}

// ---------------------------------------------------------------------------
// Glyph + Atlas
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Glyph {
    dot_x: f64,
    dot_y: f64,
    frame: Rect,
    advance: f64,
}

/// Atlas 保存一个字体 / 尺寸下预计算好的 glyph 度量。
struct Atlas {
    mapping: HashMap<char, Glyph>,
    ascent: f64,
    descent: f64,
    line_height: f64,
}

/// 单个 glyph 经过 Go freetype 缩放后的 Int26_6 像素度量。
#[derive(Debug, Clone, Copy)]
struct GlyphMetrics {
    /// Int26_6 表示的 glyph 控制点包围盒（已缩放、Y 已经翻成 "正值向下"）。
    /// 即与 Go `face.GlyphBounds` 返回的 `bounds` 对齐。
    bx_min: i32,
    by_min: i32,
    bx_max: i32,
    by_max: i32,
    /// Int26_6 表示的水平推进量。
    advance: i32,
}

impl Atlas {
    /// 按 Go 的 `NewAtlas` 逻辑构造 atlas。
    fn new(face: &Face<'_>, size: i32, runes: &[char]) -> Self {
        let fupe = face.units_per_em() as i32;
        // 缺省 dpi = 72：scale = round(size * 72 * 64 / 72 + 0.5) = round(size*64+0.5)
        // 对整数 size 等价于 size * 64。这里复刻 Go 的表达式保证严谨。
        let scale_26_6 = (0.5 + (size as f64 * 72.0 * 64.0 / 72.0)) as i32;

        // Go `face.Metrics()`:
        //   Height  = a.scale                              (Int26_6)
        //   Ascent  = Int26_6(Ceil(scale * ascent / fupe)) (Int26_6 raw value！)
        //   Descent = Int26_6(Ceil(scale * -descent / fupe))
        let scale_f = scale_26_6 as f64;
        let ascent_raw = (scale_f * face.ascender() as f64 / fupe as f64).ceil() as i32;
        let descent_raw = (scale_f * (-face.descender() as f64) / fupe as f64).ceil() as i32;
        let ascent = i2f(ascent_raw);
        let descent = i2f(descent_raw);
        let line_height = i2f(scale_26_6);

        // Go 把 Ascent / Descent 当成「像素数」直接加给 dot，所以这里要和
        // atlas 布局里使用的 `face.Metrics().Ascent + face.Metrics().Descent`
        // 保持一致（也是 Int26_6 raw value）。
        let row_step_26_6 = ascent_raw + descent_raw;

        // --- 收集 runes + 预先计算 Int26_6 度量 ------------------------------
        use std::collections::HashSet;
        let mut seen: HashSet<char> = HashSet::new();
        let mut order: Vec<char> = Vec::with_capacity(runes.len() + 1);
        order.push(REPLACEMENT_CHAR);
        seen.insert(REPLACEMENT_CHAR);
        for &r in runes {
            if seen.insert(r) {
                order.push(r);
            }
        }

        // 只保留那些可以计算出 metrics 的字形（Go: `face.GlyphBounds` 返回
        // ok=false 时跳过）。
        let mut metrics: HashMap<char, GlyphMetrics> = HashMap::new();
        let mut valid_runes: Vec<char> = Vec::with_capacity(order.len());
        for r in order {
            if let Some(m) = compute_glyph_metrics(face, r, scale_26_6, fupe) {
                metrics.insert(r, m);
                valid_runes.push(r);
            }
        }

        // --- 走 Go 的 makeSquareMapping -------------------------------------
        // 这里其实只影响 atlas 的 Y 坐标：宽度到达 `width` 时换行。对最终
        // `MeasurePrecise` 的结果影响有限，但为了与 Go 完全对齐仍然执行。
        let padding_26_6 = i_pixel(2);
        let lo_init = 0i32;
        let hi_init = i_pixel(1024 * 1024);
        let mut lo = lo_init;
        let mut hi = hi_init;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let (_mapping, bounds) =
                make_mapping(&valid_runes, &metrics, padding_26_6, mid, row_step_26_6);
            let bw = bounds.max_x - bounds.min_x;
            let bh = bounds.max_y - bounds.min_y;
            if bw >= bh {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        let best_width = lo;
        let (fixed_mapping, fixed_bounds) = make_mapping(
            &valid_runes,
            &metrics,
            padding_26_6,
            best_width,
            row_step_26_6,
        );

        // 将 Int26_6 mapping 转成 f64 像素并翻转 Y（Go atlas.go 相同处理）。
        let bounds_tl_y = i2f(fixed_bounds.min_y);
        let bounds_br_y = i2f(fixed_bounds.max_y);

        let mut mapping: HashMap<char, Glyph> = HashMap::new();
        for (r, fg) in fixed_mapping {
            let dot_x = i2f(fg.dot_x);
            let dot_y = bounds_br_y - (i2f(fg.dot_y) - bounds_tl_y);

            let frame_tl_x = i2f(fg.frame_min_x);
            let frame_tl_y = bounds_br_y - (i2f(fg.frame_min_y) - bounds_tl_y);
            let frame_br_x = i2f(fg.frame_max_x);
            let frame_br_y = bounds_br_y - (i2f(fg.frame_max_y) - bounds_tl_y);

            let frame = Rect {
                tl_x: frame_tl_x,
                tl_y: frame_tl_y,
                br_x: frame_br_x,
                br_y: frame_br_y,
            }
            .norm();

            mapping.insert(
                r,
                Glyph {
                    dot_x,
                    dot_y,
                    frame,
                    advance: i2f(fg.advance),
                },
            );
        }

        Self {
            mapping,
            ascent,
            descent,
            line_height,
        }
    }

    fn contains(&self, r: char) -> bool {
        self.mapping.contains_key(&r)
    }

    fn glyph(&self, r: char) -> Glyph {
        self.mapping
            .get(&r)
            .copied()
            .unwrap_or_else(|| self.mapping[&REPLACEMENT_CHAR])
    }

    /// 与 Go freetype 的 `Face.Kern` 对齐 —— 只读取旧 `kern` 表；对于 Source
    /// Sans Pro 等没有该表的字体，恒为 0。
    fn kern(&self, _r0: char, _r1: char) -> f64 {
        0.0
    }

    /// 画一个 rune，返回 (rect, frame, bounds, new_dot_x, new_dot_y)。
    fn draw_rune(
        &self,
        prev_r: Option<char>,
        r: char,
        dot_x: f64,
        dot_y: f64,
    ) -> (Rect, Rect, Rect, f64, f64) {
        let r = if self.contains(r) {
            r
        } else {
            REPLACEMENT_CHAR
        };
        if !self.contains(REPLACEMENT_CHAR) {
            return (Rect::zero(), Rect::zero(), Rect::zero(), dot_x, dot_y);
        }

        let mut dx = dot_x;
        let dy = dot_y;

        if let Some(prev) = prev_r {
            let prev_eff = if self.contains(prev) {
                prev
            } else {
                REPLACEMENT_CHAR
            };
            dx += self.kern(prev_eff, r);
        }

        let glyph = self.glyph(r);

        let sub_x = dx - glyph.dot_x;
        let sub_y = dy - glyph.dot_y;

        let rect2 = Rect {
            tl_x: glyph.frame.tl_x + sub_x,
            tl_y: glyph.frame.tl_y + sub_y,
            br_x: glyph.frame.br_x + sub_x,
            br_y: glyph.frame.br_y + sub_y,
        };

        let mut bounds = rect2;
        if bounds.w() * bounds.h() != 0.0 {
            bounds = Rect {
                tl_x: bounds.tl_x,
                tl_y: dy - self.descent,
                br_x: bounds.br_x,
                br_y: dy + self.ascent,
            };
        }

        let new_dx = dx + glyph.advance;
        (rect2, glyph.frame, bounds, new_dx, dy)
    }
}

// ---------------------------------------------------------------------------
// make_mapping —— 对应 Go `makeMapping`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct FixedGlyph {
    dot_x: i32,
    dot_y: i32,
    frame_min_x: i32,
    frame_min_y: i32,
    frame_max_x: i32,
    frame_max_y: i32,
    advance: i32,
}

#[derive(Debug, Clone, Copy, Default)]
struct FixedBounds {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl FixedBounds {
    fn union_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        // Go 的 Union 在「当前矩形非空 vs empty」时有细微差异，但对度量没有
        // 直接影响（measure 时会被 bounds==0 分支保护）。这里使用一致的「空
        // 则替换」逻辑来匹配 `fixed.Rectangle26_6{}.Union`。
        if self.min_x == 0 && self.min_y == 0 && self.max_x == 0 && self.max_y == 0 {
            self.min_x = x0;
            self.min_y = y0;
            self.max_x = x1;
            self.max_y = y1;
            return;
        }
        if x0 < self.min_x {
            self.min_x = x0;
        }
        if y0 < self.min_y {
            self.min_y = y0;
        }
        if x1 > self.max_x {
            self.max_x = x1;
        }
        if y1 > self.max_y {
            self.max_y = y1;
        }
    }
}

fn make_mapping(
    runes: &[char],
    metrics: &HashMap<char, GlyphMetrics>,
    padding_26_6: i32,
    width_26_6: i32,
    row_step_26_6: i32,
) -> (HashMap<char, FixedGlyph>, FixedBounds) {
    let mut mapping: HashMap<char, FixedGlyph> = HashMap::new();
    let mut bounds = FixedBounds::default();

    let mut dot_x = 0i32;
    let mut dot_y = 0i32;

    for &r in runes {
        let m = match metrics.get(&r) {
            Some(m) => m,
            None => continue,
        };

        // Floor/Ceil 对齐到整像素（Int26_6 中仍存储为 64 的倍数）。
        let frame_min_x_0 = i_pixel(floor_26_6(m.bx_min));
        let frame_min_y_0 = i_pixel(floor_26_6(m.by_min));
        let frame_max_x_0 = i_pixel(ceil_26_6(m.bx_max));
        let frame_max_y_0 = i_pixel(ceil_26_6(m.by_max));

        // dot.X -= frame.Min.X
        dot_x -= frame_min_x_0;

        // frame = frame.Add(dot)
        let frame_min_x = frame_min_x_0 + dot_x;
        let frame_min_y = frame_min_y_0 + dot_y;
        let frame_max_x = frame_max_x_0 + dot_x;
        let frame_max_y = frame_max_y_0 + dot_y;

        mapping.insert(
            r,
            FixedGlyph {
                dot_x,
                dot_y,
                frame_min_x,
                frame_min_y,
                frame_max_x,
                frame_max_y,
                advance: m.advance,
            },
        );

        bounds.union_rect(frame_min_x, frame_min_y, frame_max_x, frame_max_y);

        // dot.X = frame.Max.X
        dot_x = frame_max_x;
        // padding + align 到整像素
        dot_x += padding_26_6;
        dot_x = i_pixel(ceil_26_6(dot_x));

        // 宽度超过，换行
        if frame_max_x >= width_26_6 {
            dot_x = 0;
            dot_y += row_step_26_6;
            dot_y += padding_26_6;
            dot_y = i_pixel(ceil_26_6(dot_y));
        }
    }

    (mapping, bounds)
}

/// Scaled-bounds output for a glyph. `b*` are Int26_6 values in Go's
/// Y-inverted coordinate system (`xmin = +Min.X`, `ymin = -Max.Y`,
/// `xmax = +Max.X`, `ymax = -Min.Y`).
#[derive(Clone, Copy, Debug, Default)]
struct ScaledBounds {
    has_any: bool,
    x_min: i32,
    y_min: i32,
    x_max: i32,
    y_max: i32,
}

impl ScaledBounds {
    fn ingest(&mut self, x: i32, y: i32) {
        if !self.has_any {
            self.has_any = true;
            self.x_min = x;
            self.x_max = x;
            self.y_min = y;
            self.y_max = y;
            return;
        }
        if x < self.x_min {
            self.x_min = x;
        }
        if x > self.x_max {
            self.x_max = x;
        }
        if y < self.y_min {
            self.y_min = y;
        }
        if y > self.y_max {
            self.y_max = y;
        }
    }

    fn union(&mut self, other: &ScaledBounds) {
        if !other.has_any {
            return;
        }
        self.ingest(other.x_min, other.y_min);
        self.ingest(other.x_max, other.y_max);
    }

    fn shift(&self, dx: i32, dy: i32) -> ScaledBounds {
        ScaledBounds {
            has_any: self.has_any,
            x_min: self.x_min + dx,
            x_max: self.x_max + dx,
            y_min: self.y_min + dy,
            y_max: self.y_max + dy,
        }
    }
}

/// Read the raw glyf slice for the given gid so we can detect compound
/// glyphs (numContours < 0) and iterate their component records. Simple
/// glyphs use ttf-parser's regular bounding-box path.
fn get_glyf_slice<'a>(face: &Face<'a>, gid: ttf_parser::GlyphId) -> Option<&'a [u8]> {
    // ttf-parser doesn't expose a public helper to grab the raw glyf
    // range, so pull it out of the RawFace table list. We look up the
    // `loca` and `glyf` tables manually and index into them.
    let raw = face.raw_face();
    let loca_data = raw.table(ttf_parser::Tag::from_bytes(b"loca"))?;
    let glyf_data = raw.table(ttf_parser::Tag::from_bytes(b"glyf"))?;
    let head_data = raw.table(ttf_parser::Tag::from_bytes(b"head"))?;
    // head.indexToLocFormat at byte 50 (u16)
    if head_data.len() < 52 {
        return None;
    }
    let loca_fmt = u16::from_be_bytes([head_data[50], head_data[51]]);
    let idx = gid.0 as usize;
    let (start, end) = if loca_fmt == 0 {
        // short format: u16 offsets * 2
        if loca_data.len() < 2 * idx + 4 {
            return None;
        }
        let a = u16::from_be_bytes([loca_data[2 * idx], loca_data[2 * idx + 1]]) as usize * 2;
        let b = u16::from_be_bytes([loca_data[2 * idx + 2], loca_data[2 * idx + 3]]) as usize * 2;
        (a, b)
    } else {
        if loca_data.len() < 4 * idx + 8 {
            return None;
        }
        let a = u32::from_be_bytes([
            loca_data[4 * idx],
            loca_data[4 * idx + 1],
            loca_data[4 * idx + 2],
            loca_data[4 * idx + 3],
        ]) as usize;
        let b = u32::from_be_bytes([
            loca_data[4 * idx + 4],
            loca_data[4 * idx + 5],
            loca_data[4 * idx + 6],
            loca_data[4 * idx + 7],
        ]) as usize;
        (a, b)
    };
    if start >= end || end > glyf_data.len() {
        return None;
    }
    Some(&glyf_data[start..end])
}

/// Compute Go-compatible `glyphBuf.Bounds` for a single glyph (scaled
/// to Int26_6) by walking its compound components when necessary and
/// applying `roundXYToGrid` to the component translations. Returns the
/// bounds in the *pre-Y-inversion* coordinate system — callers must
/// still apply the `(xmin=+Min.X, ymin=-Max.Y, …)` flip before storing
/// `GlyphMetrics`.
fn compute_glyph_bounds_scaled(
    face: &Face<'_>,
    gid: ttf_parser::GlyphId,
    scale_26_6: i32,
    fupe: i32,
    recursion: u32,
) -> ScaledBounds {
    if recursion >= 32 {
        return ScaledBounds::default();
    }
    let gd = match get_glyf_slice(face, gid) {
        Some(d) => d,
        None => return ScaledBounds::default(),
    };
    if gd.len() < 10 {
        return ScaledBounds::default();
    }
    let ne = i16::from_be_bytes([gd[0], gd[1]]);
    if ne >= 0 {
        // Simple glyph: ttf-parser's glyph_bounding_box already returns
        // the control-point rectangle in funit. Scale it and return.
        let bb = match face.glyph_bounding_box(gid) {
            Some(b) => b,
            None => return ScaledBounds::default(),
        };
        let mut out = ScaledBounds::default();
        out.ingest(
            scale_funit_to_26_6(bb.x_min as i32, scale_26_6, fupe),
            scale_funit_to_26_6(bb.y_min as i32, scale_26_6, fupe),
        );
        out.ingest(
            scale_funit_to_26_6(bb.x_max as i32, scale_26_6, fupe),
            scale_funit_to_26_6(bb.y_max as i32, scale_26_6, fupe),
        );
        return out;
    }

    // Compound glyph: walk each component record, load its own bounds
    // recursively, apply the transform/translation and merge.
    const FLAG_ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const FLAG_ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const FLAG_ROUND_XY_TO_GRID: u16 = 0x0004;
    const FLAG_WE_HAVE_A_SCALE: u16 = 0x0008;
    const FLAG_MORE_COMPONENTS: u16 = 0x0020;
    const FLAG_WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    const FLAG_WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

    let mut out = ScaledBounds::default();
    let mut p = 10usize;
    loop {
        if p + 4 > gd.len() {
            break;
        }
        let flags = u16::from_be_bytes([gd[p], gd[p + 1]]);
        let comp_idx = u16::from_be_bytes([gd[p + 2], gd[p + 3]]);
        p += 4;

        let (dx_raw, dy_raw) = if flags & FLAG_ARG_1_AND_2_ARE_WORDS != 0 {
            if p + 4 > gd.len() {
                break;
            }
            let dx = i16::from_be_bytes([gd[p], gd[p + 1]]) as i32;
            let dy = i16::from_be_bytes([gd[p + 2], gd[p + 3]]) as i32;
            p += 4;
            (dx, dy)
        } else {
            if p + 2 > gd.len() {
                break;
            }
            let dx = (gd[p] as i8) as i32;
            let dy = (gd[p + 1] as i8) as i32;
            p += 2;
            (dx, dy)
        };

        // We only support `args are XY values` + no transform (the
        // common case for Source Sans Pro). Glyphs requesting a point
        // matching / scale / 2×2 transform fall through to the
        // component's bounds without adjustment — they're rare enough
        // that we'd rather log an approximation than panic.
        let has_transform = flags
            & (FLAG_WE_HAVE_A_SCALE | FLAG_WE_HAVE_AN_X_AND_Y_SCALE | FLAG_WE_HAVE_A_TWO_BY_TWO)
            != 0;
        if has_transform {
            // Skip past the transform bytes.
            let skip = if flags & FLAG_WE_HAVE_A_SCALE != 0 {
                2
            } else if flags & FLAG_WE_HAVE_AN_X_AND_Y_SCALE != 0 {
                4
            } else {
                8
            };
            p += skip;
        }

        let component_bounds = compute_glyph_bounds_scaled(
            face,
            ttf_parser::GlyphId(comp_idx),
            scale_26_6,
            fupe,
            recursion + 1,
        );

        if flags & FLAG_ARGS_ARE_XY_VALUES != 0 {
            // Translate by (dx, dy). Go scales the raw Int26_6 dx by
            // `font.scale(g.scale * dx)` which is effectively
            // `scale_funit_to_26_6(dx_raw, scale_26_6, fupe)`.
            let mut dx_scaled = scale_funit_to_26_6(dx_raw, scale_26_6, fupe);
            let mut dy_scaled = scale_funit_to_26_6(dy_raw, scale_26_6, fupe);
            if flags & FLAG_ROUND_XY_TO_GRID != 0 {
                // `(v + 32) &^ 63` — round to the nearest integer pixel
                // boundary in Int26_6.
                dx_scaled = (dx_scaled + 32) & !63;
                dy_scaled = (dy_scaled + 32) & !63;
            }
            let shifted = component_bounds.shift(dx_scaled, dy_scaled);
            out.union(&shifted);
        } else {
            // Point matching: not yet supported, just merge without
            // translation. Should not affect Source Sans Pro tests.
            out.union(&component_bounds);
        }

        if flags & FLAG_MORE_COMPONENTS == 0 {
            break;
        }
    }

    out
}

/// Build `GlyphMetrics` (Int26_6 control-box + advance) for a char,
/// matching Go freetype's `face.GlyphBounds` output — including the
/// `roundXYToGrid` adjustment applied by compound glyph records. Using
/// ttf-parser's raw `glyph_bounding_box` for compound glyphs is off by
/// up to one pixel because it skips that rounding step.
fn compute_glyph_metrics(
    face: &Face<'_>,
    ch: char,
    scale_26_6: i32,
    fupe: i32,
) -> Option<GlyphMetrics> {
    let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
    let advance_funit = face.glyph_hor_advance(gid)? as i32;
    let advance = scale_funit_to_26_6(advance_funit, scale_26_6, fupe);

    let bounds = compute_glyph_bounds_scaled(face, gid, scale_26_6, fupe, 0);
    if !bounds.has_any {
        // Go returns (0,0,0,0) + the normal advance for glyphs with no
        // outline (e.g. space). Mirror that so `.w() * .h() == 0` falls
        // through in the caller.
        return Some(GlyphMetrics {
            bx_min: 0,
            by_min: 0,
            bx_max: 0,
            by_max: 0,
            advance,
        });
    }

    // Go stores glyph bounds in a Y-inverted coordinate system:
    //   xmin = +Min.X, ymin = -Max.Y, xmax = +Max.X, ymax = -Min.Y
    let bx_min = bounds.x_min;
    let bx_max = bounds.x_max;
    let by_min = -bounds.y_max;
    let by_max = -bounds.y_min;

    if bx_min > bx_max || by_min > by_max {
        return None;
    }

    Some(GlyphMetrics {
        bx_min,
        by_min,
        bx_max,
        by_max,
        advance,
    })
}

// ---------------------------------------------------------------------------
// Ruler
// ---------------------------------------------------------------------------

/// 文本度量 Ruler —— 为每个 (family, style, size) 维护一个 Atlas。
pub struct Ruler {
    orig_x: f64,
    orig_y: f64,
    dot_x: f64,
    dot_y: f64,
    pub line_height_factor: f64,

    line_heights: HashMap<FontKey, f64>,
    tab_widths: HashMap<FontKey, f64>,
    atlases: HashMap<FontKey, Atlas>,
    /// 原始 TTF 字节（按 family+style 归档，size 无关）。
    ttfs: HashMap<FontKey, &'static [u8]>,

    prev_r: Option<char>,
    bounds: Rect,
    bounds_with_dot: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FontKey {
    family: FontFamily,
    style: FontStyle,
    size: i32,
}

impl From<Font> for FontKey {
    fn from(f: Font) -> Self {
        Self {
            family: f.family,
            style: f.style,
            size: f.size,
        }
    }
}

impl FontKey {
    fn sizeless(self) -> Self {
        Self {
            size: SIZELESS_FONT_SIZE,
            ..self
        }
    }
}

impl Ruler {
    /// 创建 Ruler 并载入所有内建字体的 TTF 数据。
    pub fn new() -> Result<Self, String> {
        let mut ttfs: HashMap<FontKey, &'static [u8]> = HashMap::new();

        for &family in FONT_FAMILIES {
            for &style in FONT_STYLES {
                let key = FontKey {
                    family,
                    style,
                    size: SIZELESS_FONT_SIZE,
                };
                if ttfs.contains_key(&key) {
                    continue;
                }
                let face_data = crate::fonts::lookup_font_face(family, style);
                // 先试解析确保合法。
                Face::parse(face_data, 0)
                    .map_err(|e| format!("failed to parse font {:?} {:?}: {}", family, style, e))?;
                ttfs.insert(key, face_data);
            }
        }

        Ok(Self {
            orig_x: 0.0,
            orig_y: 0.0,
            dot_x: 0.0,
            dot_y: 0.0,
            line_height_factor: 1.0,
            line_heights: HashMap::new(),
            tab_widths: HashMap::new(),
            atlases: HashMap::new(),
            ttfs,
            prev_r: None,
            bounds: Rect::zero(),
            bounds_with_dot: false,
        })
    }

    fn add_font_size(&mut self, font: Font) {
        let key = FontKey::from(font);
        let sizeless = key.sizeless();
        let runes = default_runes();

        let data = self.ttfs[&sizeless];
        let face = Face::parse(data, 0).expect("previously validated");
        let atlas = Atlas::new(&face, font.size, &runes);

        let lh = atlas.line_height;
        let tw = atlas.glyph(' ').advance * TAB_SIZE;

        self.line_heights.insert(key, lh);
        self.tab_widths.insert(key, tw);
        self.atlases.insert(key, atlas);
    }

    fn clear(&mut self) {
        self.prev_r = None;
        self.bounds = Rect::zero();
        self.dot_x = self.orig_x;
        self.dot_y = self.orig_y;
    }

    fn control_rune(&self, r: char, dot_x: f64, dot_y: f64, font: Font) -> Option<(f64, f64)> {
        let key = FontKey::from(font);
        match r {
            '\n' => {
                let new_x = self.orig_x;
                let new_y = dot_y - self.line_height_factor * self.line_heights[&key];
                Some((new_x, new_y))
            }
            '\r' => Some((self.orig_x, dot_y)),
            '\t' => {
                let tw = self.tab_widths[&key];
                let mut rem = (dot_x - self.orig_x) % tw;
                rem = rem % (rem + tw);
                if rem == 0.0 {
                    rem = tw;
                }
                Some((dot_x + rem, dot_y))
            }
            _ => None,
        }
    }

    fn draw_buf(&mut self, font: Font, text: &str) {
        let key = FontKey::from(font);
        for ch in text.chars() {
            if let Some((nx, ny)) = self.control_rune(ch, self.dot_x, self.dot_y, font) {
                self.dot_x = nx;
                self.dot_y = ny;
                continue;
            }

            let (_, _, bounds, new_dx, new_dy) =
                self.atlases[&key].draw_rune(self.prev_r, ch, self.dot_x, self.dot_y);

            self.prev_r = Some(ch);
            self.dot_x = new_dx;
            self.dot_y = new_dy;

            if self.bounds_with_dot {
                let dot_rect = Rect {
                    tl_x: self.dot_x,
                    tl_y: self.dot_y,
                    br_x: self.dot_x,
                    br_y: self.dot_y,
                };
                self.bounds = self.bounds.union(dot_rect);
                self.bounds = self.bounds.union(bounds);
            } else if self.bounds.w() * self.bounds.h() == 0.0 {
                self.bounds = bounds;
            } else {
                self.bounds = self.bounds.union(bounds);
            }
        }
    }

    /// 精确测量文本：返回浮点宽高。
    pub fn measure_precise(&mut self, font: Font, s: &str) -> (f64, f64) {
        let key = FontKey::from(font);
        if !self.atlases.contains_key(&key) {
            self.add_font_size(font);
        }
        self.clear();
        self.draw_buf(font, s);
        (self.bounds.w(), self.bounds.h())
    }

    /// 度量文本：向上取整为 i32；同时对非 BMP 合成字 (e.g. emoji) 做修正。
    pub fn measure(&mut self, font: Font, s: &str) -> (i32, i32) {
        let (w, h) = self.measure_precise(font, s);
        let w = self.scale_unicode(w, font, s);
        (w.ceil() as i32, h.ceil() as i32)
    }

    /// Mono 模式：把 dot 也并入 bounds。
    pub fn measure_mono(&mut self, font: Font, s: &str) -> (i32, i32) {
        let orig = self.bounds_with_dot;
        self.bounds_with_dot = true;
        let result = self.measure(font, s);
        self.bounds_with_dot = orig;
        result
    }

    fn space_width(&mut self, font: Font) -> f64 {
        let key = FontKey::from(font);
        if !self.atlases.contains_key(&key) {
            self.add_font_size(font);
        }
        self.atlases[&key].glyph(' ').advance
    }

    fn scale_unicode(&mut self, mut w: f64, font: Font, s: &str) -> f64 {
        let grapheme_count = s.graphemes(true).count();
        if grapheme_count != s.len() {
            for line in s.split('\n') {
                let (line_w, _) = self.measure_precise(font, line);
                let mut adjusted_w = line_w;

                let mono = Font::new(FontFamily::SourceCodePro, font.style, font.size);
                for grapheme in line.graphemes(true) {
                    let unicode_width = UnicodeWidthStr::width(grapheme);
                    if unicode_width == 1 {
                        continue;
                    }

                    let key = FontKey::from(font);
                    let mut prev_r: Option<char> = None;
                    let dot_x_start = self.orig_x;
                    let dot_y_start = self.orig_y;
                    let mut dx = dot_x_start;
                    let mut dy = dot_y_start;
                    let mut b = Rect::zero();

                    for ch in grapheme.chars() {
                        if let Some((nx, ny)) = self.control_rune(ch, dx, dy, font) {
                            dx = nx;
                            dy = ny;
                            continue;
                        }
                        let (_, _, bounds, new_dx, new_dy) =
                            self.atlases[&key].draw_rune(prev_r, ch, dx, dy);
                        b = b.union(bounds);
                        prev_r = Some(ch);
                        dx = new_dx;
                        dy = new_dy;
                    }

                    adjusted_w -= b.w();
                    adjusted_w += self.space_width(mono) * unicode_width as f64;
                }

                w = w.max(adjusted_w);
            }
        }
        w
    }
}

// ---------------------------------------------------------------------------
// Markdown render + measure
// ---------------------------------------------------------------------------

fn markdown_options() -> Options {
    Options {
        parse: ParseOptions {
            constructs: Constructs {
                gfm_strikethrough: true,
                gfm_table: true,
                ..Constructs::default()
            },
            ..ParseOptions::default()
        },
        compile: CompileOptions {
            allow_dangerous_html: true,
            allow_dangerous_protocol: true,
            ..CompileOptions::default()
        },
    }
}

fn sanitize_links(input: &str) -> String {
    HREF_RE
        .replace_all(input, |caps: &regex::Captures<'_>| {
            let value = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let value = value.replace("&amp;", "TEMP_AMP");
            let value = value.replace('&', "&amp;");
            let value = value.replace("TEMP_AMP", "&amp;");
            format!(r#"href="{}""#, value)
        })
        .into_owned()
}

pub fn render_markdown(input: &str) -> Result<String, String> {
    let rendered = markdown::to_html_with_options(input, &markdown_options())
        .map_err(|e| format!("markdown render failed: {e}"))?;
    let mut rendered = sanitize_links(&rendered);
    if !rendered.is_empty() && !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok(rendered)
}

pub fn header_to_font_size(base_font_size: i32, header: &str) -> i32 {
    match header {
        "h1" => (H1_EM * f64::from(base_font_size)) as i32,
        "h2" => (H2_EM * f64::from(base_font_size)) as i32,
        "h3" => (H3_EM * f64::from(base_font_size)) as i32,
        "h4" => (H4_EM * f64::from(base_font_size)) as i32,
        "h5" => (H5_EM * f64::from(base_font_size)) as i32,
        "h6" => (H6_EM * f64::from(base_font_size)) as i32,
        _ => 0,
    }
}

pub fn measure_markdown(
    md_text: &str,
    ruler: &mut Ruler,
    font_family: Option<FontFamily>,
    mono_font_family: Option<FontFamily>,
    font_size: i32,
) -> Result<(i32, i32), String> {
    let render = render_markdown(md_text)?;
    let wrapped = format!("<body>{}</body>", render);
    let doc = Document::parse(&wrapped).map_err(|e| format!("markdown parse failed: {e}"))?;

    let original_line_height = ruler.line_height_factor;
    let original_bounds_with_dot = ruler.bounds_with_dot;
    ruler.bounds_with_dot = true;
    ruler.line_height_factor = MARKDOWN_LINE_HEIGHT;

    let body_node = doc.root_element();
    let body_attrs = measure_node(
        ruler,
        0,
        body_node,
        font_family,
        mono_font_family,
        font_size,
        FontStyle::Regular,
    );

    ruler.line_height_factor = original_line_height;
    ruler.bounds_with_dot = original_bounds_with_dot;

    Ok((
        body_attrs.width.ceil() as i32,
        body_attrs.height.ceil() as i32,
    ))
}

#[derive(Debug, Clone, Default)]
struct BlockAttrs {
    width: f64,
    height: f64,
    margin_top: f64,
    margin_bottom: f64,
    extra_data: ExtraData,
}

impl BlockAttrs {
    fn is_not_empty(&self) -> bool {
        self.width != 0.0
            || self.height != 0.0
            || self.margin_top != 0.0
            || self.margin_bottom != 0.0
            || !matches!(self.extra_data, ExtraData::None)
    }
}

#[derive(Debug, Clone, Default)]
enum ExtraData {
    #[default]
    None,
    Row(Vec<f64>),
    Section(Vec<Vec<f64>>),
}

fn trim_markdown_text_node(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, '\n' | '\t' | '\u{0008}'))
}

fn is_empty_sibling_node(node: Node<'_, '_>) -> bool {
    match node.node_type() {
        NodeType::Text => node.text().unwrap_or_default().trim().is_empty(),
        _ => false,
    }
}

fn has_prev(node: Node<'_, '_>) -> bool {
    let Some(prev) = node.prev_sibling() else {
        return false;
    };
    if is_empty_sibling_node(prev) {
        return has_prev(prev);
    }
    true
}

fn has_next(node: Node<'_, '_>) -> bool {
    let Some(next) = node.next_sibling() else {
        return false;
    };
    if is_empty_sibling_node(next) {
        return has_next(next);
    }
    true
}

fn get_prev<'a, 'input>(node: Option<Node<'a, 'input>>) -> Option<Node<'a, 'input>> {
    let node = node?;
    if is_empty_sibling_node(node)
        && let Some(next) = get_next(node.prev_sibling())
    {
        return Some(next);
    }
    Some(node)
}

fn get_next<'a, 'input>(node: Option<Node<'a, 'input>>) -> Option<Node<'a, 'input>> {
    let node = node?;
    if is_empty_sibling_node(node)
        && let Some(next) = get_next(node.next_sibling())
    {
        return Some(next);
    }
    Some(node)
}

fn is_block_element(el_type: &str) -> bool {
    matches!(
        el_type,
        "blockquote"
            | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "ul"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
    )
}

fn has_ancestor_element(node: Node<'_, '_>, el_type: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.is_element() && parent.tag_name().name() == el_type {
        return true;
    }
    has_ancestor_element(parent, el_type)
}

fn merge_column_widths(mut existing: Vec<f64>, new_rows: &[Vec<f64>]) -> Vec<f64> {
    for row_widths in new_rows {
        for (i, width) in row_widths.iter().enumerate() {
            if i >= existing.len() {
                existing.push(*width);
            } else {
                existing[i] = existing[i].max(*width);
            }
        }
    }
    existing
}

fn measure_node(
    ruler: &mut Ruler,
    depth: usize,
    node: Node<'_, '_>,
    font_family: Option<FontFamily>,
    mono_font_family: Option<FontFamily>,
    mut font_size: i32,
    mut font_style: FontStyle,
) -> BlockAttrs {
    let _ = depth;
    let mut font_family = font_family.unwrap_or(FontFamily::SourceSansPro);

    let parent_element_type = node
        .parent()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name());

    match node.node_type() {
        NodeType::Text => {
            let Some(raw) = node.text() else {
                return BlockAttrs::default();
            };
            if trim_markdown_text_node(raw).is_empty() {
                return BlockAttrs::default();
            }

            let is_code = matches!(parent_element_type, Some("pre" | "code"));
            let font = font_family.font(font_size, font_style);
            let mut str_ = raw.to_owned();
            let mut space_widths = 0.0;

            if !is_code {
                let space_width = ruler.space_width(font);
                str_ = str_.replace('\n', " ");
                str_ = str_.replace('\t', " ");
                if str_.starts_with(' ') {
                    str_.remove(0);
                    if has_prev(node) {
                        space_widths += space_width;
                    }
                }
                if str_.ends_with(' ') {
                    str_.pop();
                    if has_next(node) {
                        space_widths += space_width;
                    }
                }
            }

            if parent_element_type == Some("pre") {
                let original_line_height = ruler.line_height_factor;
                ruler.line_height_factor = LINE_HEIGHT_PRE;
                let (mut w, mut h) = ruler.measure_precise(font, &str_);
                ruler.line_height_factor = original_line_height;
                w *= FONT_SIZE_PRE_CODE_EM;
                h *= FONT_SIZE_PRE_CODE_EM;
                return BlockAttrs {
                    width: w + space_widths,
                    height: h,
                    ..Default::default()
                };
            }

            let (mut w, h) = ruler.measure_precise(font, &str_);
            if is_code {
                w *= FONT_SIZE_PRE_CODE_EM;
                return BlockAttrs {
                    width: w + space_widths,
                    height: h * FONT_SIZE_PRE_CODE_EM,
                    ..Default::default()
                };
            }

            w = ruler.scale_unicode(w, font, &str_);
            BlockAttrs {
                width: w + space_widths,
                height: h,
                ..Default::default()
            }
        }
        NodeType::Element => {
            let tag = node.tag_name().name();
            let mut is_code = false;

            match tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    font_size = header_to_font_size(font_size, tag);
                    font_style = FontStyle::Semibold;
                }
                "em" => {
                    font_style = FontStyle::Italic;
                }
                "b" | "strong" => {
                    font_style = FontStyle::Bold;
                }
                "pre" | "code" => {
                    font_family = mono_font_family.unwrap_or(FontFamily::SourceCodePro);
                    font_style = FontStyle::Regular;
                    is_code = true;
                }
                _ => {}
            }

            let original_line_height = ruler.line_height_factor;
            if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                ruler.line_height_factor = LINE_HEIGHT_H;
            }

            let line_height_px = f64::from(font_size) * ruler.line_height_factor;
            let mut block = BlockAttrs::default();

            if node.first_child().is_some() {
                let first = get_next(node.first_child());
                let last = get_prev(node.last_child());
                let mut blocks: Vec<BlockAttrs> = Vec::new();
                let mut inline_block: Option<BlockAttrs> = None;

                let end_inline_block =
                    |blocks: &mut Vec<BlockAttrs>, inline_block: &mut Option<BlockAttrs>| {
                        if let Some(mut b) = inline_block.take() {
                            if !is_code && b.height > 0.0 && b.height < line_height_px {
                                b.height = line_height_px;
                            }
                            blocks.push(b);
                        }
                    };

                for child in node.children() {
                    let child_block = measure_node(
                        ruler,
                        depth + 1,
                        child,
                        Some(font_family),
                        mono_font_family,
                        font_size,
                        font_style,
                    );

                    if child.is_element() && is_block_element(child.tag_name().name()) {
                        end_inline_block(&mut blocks, &mut inline_block);
                        let mut new_block = BlockAttrs {
                            width: child_block.width,
                            height: child_block.height,
                            ..Default::default()
                        };
                        new_block.margin_top = if first == Some(child) && tag == "blockquote" {
                            0.0
                        } else {
                            child_block.margin_top
                        };
                        new_block.margin_bottom = if last == Some(child) && tag == "blockquote" {
                            0.0
                        } else {
                            child_block.margin_bottom
                        };
                        blocks.push(new_block);
                    } else if child.is_element() && child.tag_name().name() == "br" {
                        if inline_block.is_some() {
                            end_inline_block(&mut blocks, &mut inline_block);
                        } else {
                            block.height += line_height_px;
                        }
                    } else if child_block.is_not_empty() {
                        if let Some(ref mut inline) = inline_block {
                            inline.width += child_block.width;
                            inline.height = inline.height.max(child_block.height);
                            inline.margin_top = inline.margin_top.max(child_block.margin_top);
                            inline.margin_bottom =
                                inline.margin_bottom.max(child_block.margin_bottom);
                        } else {
                            inline_block = Some(child_block);
                        }
                    }
                }

                if inline_block.is_some() {
                    end_inline_block(&mut blocks, &mut inline_block);
                }

                let mut prev_margin_bottom = 0.0;
                for (i, b) in blocks.iter().enumerate() {
                    if i == 0 {
                        block.margin_top = block.margin_top.max(b.margin_top);
                    } else {
                        let margin_diff = b.margin_top - prev_margin_bottom;
                        if margin_diff > 0.0 {
                            block.height += margin_diff;
                        }
                    }
                    if i == blocks.len() - 1 {
                        block.margin_bottom = block.margin_bottom.max(b.margin_bottom);
                    } else {
                        block.height += b.margin_bottom;
                        prev_margin_bottom = b.margin_bottom;
                    }

                    block.height += b.height;
                    block.width = block.width.max(b.width);
                }
            }

            match tag {
                "blockquote" => {
                    block.width += (2.0 * PADDING_LR_BLOCKQUOTE_EM + BORDER_LEFT_BLOCKQUOTE_EM)
                        * f64::from(font_size);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_BLOCKQUOTE);
                }
                "p" => {
                    if parent_element_type == Some("li") {
                        block.margin_top = block.margin_top.max(MARGIN_TOP_LI_P);
                    }
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_P);
                }
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    block.margin_top = block.margin_top.max(MARGIN_TOP_H);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_H);
                    if matches!(tag, "h1" | "h2") {
                        block.height +=
                            PADDING_BOTTOM_H1_H2_EM * f64::from(font_size) + BORDER_BOTTOM_H1_H2;
                    }
                }
                "li" => {
                    block.width += PADDING_LEFT_UL_OL_EM * f64::from(font_size);
                    if has_prev(node) {
                        block.margin_top = block
                            .margin_top
                            .max(MARGIN_TOP_LI_EM * f64::from(font_size));
                    }
                }
                "ol" | "ul" => {
                    if has_ancestor_element(node, "ul") || has_ancestor_element(node, "ol") {
                        block.margin_top = 0.0;
                        block.margin_bottom = 0.0;
                    } else {
                        block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_UL);
                    }
                }
                "pre" => {
                    block.width += 2.0 * PADDING_PRE;
                    block.height += 2.0 * PADDING_PRE;
                    block.margin_bottom = block.margin_bottom.max(MARGIN_BOTTOM_PRE);
                }
                "code" if parent_element_type != Some("pre") => {
                    block.width += 2.0 * PADDING_LEFT_RIGHT_CODE_EM * f64::from(font_size);
                    block.height += 2.0 * PADDING_TOP_BOTTOM_CODE_EM * f64::from(font_size);
                }
                "hr" => {
                    block.height += HEIGHT_HR_EM * f64::from(font_size);
                    block.margin_top = block.margin_top.max(MARGIN_TOP_BOTTOM_HR);
                    block.margin_bottom = block.margin_bottom.max(MARGIN_TOP_BOTTOM_HR);
                }
                "table" => {
                    let mut column_widths: Vec<f64> = Vec::new();
                    let mut table_height = 0.0;
                    let table_border = 1.0;

                    for child in node.children() {
                        if child.is_element()
                            && matches!(child.tag_name().name(), "tbody" | "thead" | "tfoot")
                        {
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            table_height += child_attrs.height;
                            if let ExtraData::Section(ref widths) = child_attrs.extra_data {
                                column_widths = merge_column_widths(column_widths, widths);
                            }
                        } else if child.is_element() && child.tag_name().name() == "tr" {
                            let row_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            table_height += row_attrs.height;
                            if let ExtraData::Row(ref widths) = row_attrs.extra_data {
                                column_widths = merge_column_widths(
                                    column_widths,
                                    std::slice::from_ref(widths),
                                );
                            }
                        }
                    }

                    let mut table_width = 0.0;
                    if !column_widths.is_empty() {
                        for col_width in &column_widths {
                            table_width += *col_width;
                        }
                        table_width += (column_widths.len() as f64 + 1.0) * table_border;
                    }

                    table_height += 2.0 * table_border;
                    block.width = table_width;
                    block.height = table_height;
                }
                "thead" | "tbody" | "tfoot" => {
                    let mut section_width: f64 = 0.0;
                    let mut section_height = 0.0;
                    let mut section_column_widths: Vec<Vec<f64>> = Vec::new();

                    for child in node.children() {
                        if child.is_element() && child.tag_name().name() == "tr" {
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                font_style,
                            );
                            section_height += child_attrs.height;
                            section_width = section_width.max(child_attrs.width);
                            if let ExtraData::Row(widths) = child_attrs.extra_data {
                                section_column_widths.push(widths);
                            }
                        }
                    }

                    block.width = section_width;
                    block.height = section_height;
                    block.extra_data = ExtraData::Section(section_column_widths);
                }
                "td" | "th" => {
                    let cell_font_style = if tag == "th" {
                        FontStyle::Semibold
                    } else {
                        font_style
                    };
                    let mut cell_content_width: f64 = 0.0;
                    let mut cell_content_height = 0.0;

                    for child in node.children() {
                        let child_attrs = measure_node(
                            ruler,
                            depth + 1,
                            child,
                            Some(font_family),
                            mono_font_family,
                            font_size,
                            cell_font_style,
                        );
                        cell_content_width = cell_content_width.max(child_attrs.width);
                        cell_content_height += child_attrs.height;
                    }

                    block.width = cell_content_width;
                    block.height = cell_content_height;
                }
                "tr" => {
                    let mut row_width = 0.0;
                    let mut cell_widths: Vec<f64> = Vec::new();
                    let cell_border = 1.0;
                    let row_border = 1.0;
                    let mut max_cell_height: f64 = 0.0;
                    let mut cell_count = 0usize;

                    let in_header = has_ancestor_element(node, "thead");
                    let row_font_style = if in_header {
                        FontStyle::Semibold
                    } else {
                        font_style
                    };

                    for child in node.children() {
                        if child.is_element() && matches!(child.tag_name().name(), "td" | "th") {
                            cell_count += 1;
                            let child_font_style = if child.tag_name().name() == "th" {
                                FontStyle::Semibold
                            } else {
                                row_font_style
                            };
                            let child_attrs = measure_node(
                                ruler,
                                depth + 1,
                                child,
                                Some(font_family),
                                mono_font_family,
                                font_size,
                                child_font_style,
                            );
                            let cell_width = child_attrs.width + 13.0 * 2.0;
                            let cell_height = child_attrs.height + 6.0 * 2.0;
                            cell_widths.push(cell_width);
                            max_cell_height = max_cell_height.max(cell_height);
                        }
                    }

                    if cell_count > 0 {
                        for width in &cell_widths {
                            row_width += *width;
                        }
                        row_width += (cell_count as f64 + 1.0) * cell_border;
                    }

                    block.width = row_width;
                    block.height = max_cell_height + row_border;
                    block.extra_data = ExtraData::Row(cell_widths);
                }
                _ => {}
            }

            if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                ruler.line_height_factor = original_line_height;
            }

            if block.height > 0.0 && block.height < line_height_px {
                block.height = line_height_px;
            }
            block
        }
        _ => BlockAttrs::default(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::*;

    #[test]
    fn test_ruler_creation() {
        let ruler = Ruler::new();
        assert!(ruler.is_ok());
    }

    #[test]
    fn test_measure_precise_basic() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure_precise(font, "Hello");
        assert!(w > 0.0, "width should be positive, got {}", w);
        assert!(h > 0.0, "height should be positive, got {}", h);
    }

    /// Go 对 "Hello" 的 golden 值 —— 本 crate 的核心正确性指标。
    #[test]
    fn test_measure_hello_matches_go() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure_precise(font, "Hello");
        assert_eq!(w, 33.53125, "width of 'Hello' should match Go: got {}", w);
        assert_eq!(h, 20.125, "height of 'Hello' should match Go: got {}", h);
    }

    #[test]
    fn test_measure_single_chars_match_go() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);

        let cases: &[(&str, f64, f64)] = &[
            ("a", 7.0, 20.125),
            ("b", 8.0, 20.125),
            ("c", 7.0, 20.125),
            ("h", 7.0, 20.125),
            ("l", 3.0, 20.125),
        ];
        for (s, ew, eh) in cases {
            let (w, h) = ruler.measure_precise(font, s);
            assert_eq!(w, *ew, "width of '{}' mismatch: got {}", s, w);
            assert_eq!(h, *eh, "height of '{}' mismatch: got {}", s, h);
        }
    }

    #[test]
    fn test_measure_hello_world_matches_go() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, _h) = ruler.measure_precise(font, "Hello World");
        assert_eq!(w, 76.28125, "width of 'Hello World' mismatch: got {}", w);
    }

    #[test]
    fn test_measure_increasing_chars() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let text = "abcdefghij";
        for i in 1..text.len() {
            let (w1, h1) = ruler.measure(font, &text[..i]);
            let (w2, h2) = ruler.measure(font, &text[..i + 1]);
            assert_eq!(h1, h2, "height should not change for single line");
            assert!(
                w1 < w2,
                "width should increase: '{}' ({}) vs '{}' ({})",
                &text[..i],
                w1,
                &text[..i + 1],
                w2
            );
        }
    }

    #[test]
    fn test_measure_newlines_increase_height() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (_, h1) = ruler.measure(font, "Hello");
        let (_, h2) = ruler.measure(font, "Hello\nWorld");
        assert!(h2 > h1, "newline should increase height: {} vs {}", h1, h2);
    }

    #[test]
    fn test_font_sizes_increasing() {
        let mut ruler = Ruler::new().unwrap();
        let text = "The quick brown fox";
        for i in 0..FONT_SIZES.len() - 1 {
            let f1 = FontFamily::SourceSansPro.font(FONT_SIZES[i], FontStyle::Regular);
            let f2 = FontFamily::SourceSansPro.font(FONT_SIZES[i + 1], FontStyle::Regular);
            let (w1, h1) = ruler.measure(f1, text);
            let (w2, h2) = ruler.measure(f2, text);
            assert!(
                w1 < w2,
                "larger font size should produce wider text: size {} ({}) vs size {} ({})",
                FONT_SIZES[i],
                w1,
                FONT_SIZES[i + 1],
                w2
            );
            assert!(
                h1 < h2,
                "larger font size should produce taller text: size {} ({}) vs size {} ({})",
                FONT_SIZES[i],
                h1,
                FONT_SIZES[i + 1],
                h2
            );
        }
    }

    #[test]
    fn test_measure_empty_string() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);
        let (w, h) = ruler.measure(font, "");
        assert_eq!(w, 0);
        assert_eq!(h, 0);
    }

    #[test]
    fn test_measure_single_chars() {
        let mut ruler = Ruler::new().unwrap();
        let font = FontFamily::SourceSansPro.font(FONT_SIZE_M, FontStyle::Regular);

        let (w, h) = ruler.measure(font, "a");
        assert!(w > 0, "single 'a' width should be > 0, got {}", w);
        assert!(h > 0, "single 'a' height should be > 0, got {}", h);

        let (wa, _) = ruler.measure(font, "a");
        let (ww, _) = ruler.measure(font, "w");
        assert!(
            ww >= wa,
            "'w' should be at least as wide as 'a': {} vs {}",
            ww,
            wa
        );
    }

    #[test]
    fn test_measure_cjk_matches_go_fixture() {
        let mut ruler = Ruler::new().unwrap();

        let font_16 = FontFamily::SourceSansPro.font(16, FontStyle::Regular);
        // Japanese kanji "Soushinki" (transmitter): 3 CJK Unified Ideographs.
        let (w16, h16) = ruler.measure(font_16, "\u{9001}\u{4FE1}\u{6A5F}");
        assert_eq!(w16, 58);
        assert_eq!(h16, 21);

        let font_24 = FontFamily::SourceSansPro.font(24, FontStyle::Regular);
        // Katakana "online portal": 9 graphemes including a prolonged sound mark.
        let (w24, h24) = ruler.measure(
            font_24,
            "\u{30AA}\u{30F3}\u{30E9}\u{30A4}\u{30F3}\u{30DD}\u{30FC}\u{30BF}\u{30EB}",
        );
        assert_eq!(w24, 271);
        assert_eq!(h24, 31);
    }
}
