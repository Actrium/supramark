//! Gantt layout — compute task bar geometry, time axis, and viewBox.
//!
//! Upstream reference: `packages/mermaid/src/diagrams/gantt/{ganttRenderer.js, ganttDb.js}`.
//!
//! The renderer in upstream is normally driven by a real DOM: it reads
//! `elem.parentElement.offsetWidth` to size the chart. Under the test
//! harness (jsdom / our headless reference run) that property is 0, so
//! the SVG width is 0, the time scale's range collapses to `[0, -150]`,
//! and most coordinates end up negative. The reference SVGs preserve
//! exactly that; we replicate it bit-for-bit here.

use crate::error::Result;
use crate::model::gantt::{GanttDiagram, Task};
use crate::theme::ThemeVariables;

/// Default config values from upstream `defaultConfig.ts` / schema.
pub(crate) const TITLE_TOP_MARGIN: i32 = 25;
pub(crate) const BAR_HEIGHT: i32 = 20;
pub(crate) const BAR_GAP: i32 = 4;
pub(crate) const TOP_PADDING: i32 = 50;
pub(crate) const RIGHT_PADDING: i32 = 75;
pub(crate) const LEFT_PADDING: i32 = 75;
pub(crate) const GRID_LINE_START_PADDING: i32 = 35;
pub(crate) const FONT_SIZE: i32 = 11;
pub(crate) const SECTION_FONT_SIZE: i32 = 11;
pub(crate) const NUMBER_SECTION_STYLES: i32 = 4;

/// Resolved task with absolute times in milliseconds since epoch.
#[derive(Debug, Clone)]
pub struct ResolvedTask {
    pub id: String,
    pub name: String,
    pub start_ms: f64,
    pub end_ms: f64,
    /// Possibly different from end_ms if `checkTaskDates` adjusted it.
    pub render_end_ms: Option<f64>,
    pub done: bool,
    pub active: bool,
    pub critical: bool,
    pub milestone: bool,
    pub vert: bool,
    pub section_idx: usize,
    /// The `type` (section name) used for coloring.
    pub section_name: String,
    pub classes: Vec<String>,
    /// 0-based serial order across all tasks in input order.
    pub order: usize,
}

#[derive(Debug, Clone)]
pub struct ExcludeRange {
    /// Day in `YYYY-MM-DD` form for ID generation.
    pub start_iso: String,
    /// Time-scaled to ms (start of day).
    pub start_ms: f64,
    /// Time-scaled to ms (end of day, i.e. start of next day - 1ms).
    pub end_ms: f64,
}

#[derive(Debug, Clone)]
pub struct AxisTick {
    /// Position in time-scale ms (absolute).
    pub time_ms: f64,
    /// Formatted label.
    pub label: String,
}

/// Full gantt layout ready for rendering.
#[derive(Debug, Clone, Default)]
pub struct GanttLayout {
    /// width = 0 (matches reference output under jsdom).
    pub width: i32,
    /// total height including padding.
    pub height: i32,
    pub tasks: Vec<ResolvedTask>,
    /// Sorted, unique categories (== section names) in encounter order.
    pub categories: Vec<String>,
    /// Per-category number of rows (compact mode 1+, normal mode count).
    pub category_heights: Vec<(String, i32)>,
    pub min_time_ms: f64,
    pub max_time_ms: f64,
    pub axis_format: String,
    pub axis_ticks: Vec<AxisTick>,
    pub exclude_ranges: Vec<ExcludeRange>,
    pub today_marker: TodayMarker,
}

#[derive(Debug, Clone, Default)]
pub enum TodayMarker {
    #[default]
    DefaultLine,
    Off,
    Styled(String),
}

pub fn layout(d: &GanttDiagram, _theme: &ThemeVariables) -> Result<GanttLayout> {
    let mut layout = GanttLayout {
        width: 0,
        ..Default::default()
    };

    // Resolve tasks (date / duration / after / until).
    let resolved = resolve_tasks(d);

    // Sort by start time (stable) for rendering — upstream calls
    // `taskArray.sort(taskCompare)` which only compares startTime.
    let mut tasks = resolved;
    tasks.sort_by(|a, b| a.start_ms.partial_cmp(&b.start_ms).unwrap_or(std::cmp::Ordering::Equal));

    // Compute categories (unique section names in first-encounter order).
    let mut categories: Vec<String> = Vec::new();
    for t in &tasks {
        if !categories.iter().any(|c| c == &t.section_name) {
            categories.push(t.section_name.clone());
        }
    }

    // Compute category heights and total height.
    // (Compact mode unsupported for byte-exact target; only the few
    // fixtures that opt in via `displayMode compact` would need it.)
    let mut category_heights: Vec<(String, i32)> = Vec::new();
    for cat in &categories {
        let count = tasks.iter().filter(|t| &t.section_name == cat).count() as i32;
        category_heights.push((cat.clone(), count));
    }

    let h = 2 * TOP_PADDING + (tasks.len() as i32) * (BAR_HEIGHT + BAR_GAP);

    // Time domain.
    let (min_ms, max_ms) = if tasks.is_empty() {
        (0.0, 0.0)
    } else {
        let mut mn = f64::INFINITY;
        let mut mx = f64::NEG_INFINITY;
        for t in &tasks {
            if t.start_ms < mn {
                mn = t.start_ms;
            }
            if t.end_ms > mx {
                mx = t.end_ms;
            }
        }
        (mn, mx)
    };

    // Axis format.
    let axis_format = if let Some(fmt) = d.axis_format.as_deref() {
        fmt.to_string()
    } else if d.date_format.trim() == "D" {
        "%d".to_string()
    } else {
        "%Y-%m-%d".to_string()
    };

    // Tick generation — d3-axis defaults to ~10 ticks. Pick a daily/etc
    // interval whose count is closest to 10.
    let axis_ticks = generate_ticks(min_ms, max_ms, &axis_format, &d.date_format);

    // Exclude ranges.
    let exclude_ranges = if d.excludes.is_empty() && d.includes.is_empty() {
        Vec::new()
    } else {
        compute_exclude_ranges(min_ms, max_ms, d)
    };

    // Today marker.
    let today_marker = match d.today_marker.as_deref() {
        Some("off") => TodayMarker::Off,
        Some("") | None => TodayMarker::DefaultLine,
        Some(s) => TodayMarker::Styled(s.to_string()),
    };

    layout.height = h;
    layout.tasks = tasks;
    layout.categories = categories;
    layout.category_heights = category_heights;
    layout.min_time_ms = min_ms;
    layout.max_time_ms = max_ms;
    layout.axis_format = axis_format;
    layout.axis_ticks = axis_ticks;
    layout.exclude_ranges = exclude_ranges;
    layout.today_marker = today_marker;

    Ok(layout)
}

// ── Time scale ───────────────────────────────────────────────────────

/// d3 `scaleTime().rangeRound([0, -150])` — linearly interpolates from
/// (min..max) to (0..-150), then rounds to integer.
pub fn time_scale(value_ms: f64, min_ms: f64, max_ms: f64, width: i32) -> i32 {
    // range = [0, w - leftPadding - rightPadding] with the upstream
    // values; for w=0 that's [0, -150].
    let range_max = (width - LEFT_PADDING - RIGHT_PADDING) as f64;
    let range_min = 0.0;
    if (max_ms - min_ms).abs() < f64::EPSILON {
        return range_min as i32;
    }
    let t = (value_ms - min_ms) / (max_ms - min_ms);
    // d3 rangeRound — `Math.round`: half-up for positive, half-up
    // toward +infinity. Rust f64::round rounds half away from zero.
    // For our negative-output case we mimic JS: Math.round(-0.5) = 0,
    // Math.round(-1.5) = -1. So we use floor(x + 0.5).
    let raw = range_min + t * (range_max - range_min);
    js_round(raw)
}

/// JS `Math.round` semantics: round half toward positive infinity.
fn js_round(x: f64) -> i32 {
    (x + 0.5).floor() as i32
}

// ── Date / duration parsing ───────────────────────────────────────────

/// Parse a datetime string against a dayjs-style format.
///
/// Supported formats:
/// - `YYYY-MM-DD`
/// - `YYYY-MM-DDTHH:mm:ss` and friends
/// - `D` (numeric day of month — used as a tiny test fixture)
/// - `x` (millisecond timestamp)
/// - `X` (second timestamp)
/// - `ss` (seconds since epoch start of "00")
/// - `SSS` (milliseconds, treated as ms since "000")
///
/// Returns ms-since-epoch.
fn parse_date(s: &str, fmt: &str) -> Option<f64> {
    let s = s.trim();
    let fmt = fmt.trim();

    // Timestamp formats.
    if fmt == "x" {
        if let Ok(n) = s.parse::<f64>() {
            return Some(n);
        }
    }
    if fmt == "X" {
        if let Ok(n) = s.parse::<f64>() {
            return Some(n * 1000.0);
        }
    }
    // Trivial numeric formats used by some test fixtures.
    if fmt == "ss" {
        if let Ok(n) = s.parse::<f64>() {
            return Some(n * 1000.0);
        }
    }
    if fmt == "SSS" {
        if let Ok(n) = s.parse::<f64>() {
            return Some(n);
        }
    }
    if fmt == "D" {
        if let Ok(n) = s.parse::<f64>() {
            // Map day-N to 1970-01-N so `%d` formatting yields the
            // input value zero-padded.
            return Some((n - 1.0) * 86_400_000.0);
        }
    }

    // Strict `YYYY-MM-DD` (10 chars, hyphens at positions 4 and 7).
    if fmt == "YYYY-MM-DD" {
        return parse_iso_date(s);
    }

    // Some common compounds with time component.
    // We don't attempt to support all dayjs formats; only the ones the
    // fixtures use. Fallback: try ISO date.
    if let Some(t) = parse_iso_date(s) {
        return Some(t);
    }
    None
}

/// Parse `YYYY-MM-DD` into ms since unix epoch (UTC).
fn parse_iso_date(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = std::str::from_utf8(&bytes[0..4]).ok()?.parse().ok()?;
    let m: u32 = std::str::from_utf8(&bytes[5..7]).ok()?.parse().ok()?;
    let d: u32 = std::str::from_utf8(&bytes[8..10]).ok()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(date_to_ms(y, m, d, 0, 0, 0, 0))
}

/// Convert Y-M-D-h-m-s-ms to ms since epoch (UTC).
fn date_to_ms(y: i32, m: u32, d: u32, h: u32, mi: u32, se: u32, ms: u32) -> f64 {
    // Use the algorithm from "Howard Hinnant's date library": days from
    // civil date to days since 1970-01-01.
    let yy: i64 = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let era: i64 = if yy >= 0 { yy / 400 } else { (yy - 399) / 400 };
    let yoe: i64 = yy - era * 400;
    let doy: i64 = (153 * (if m > 2 { m - 3 } else { m + 9 } as i64) + 2) / 5 + d as i64 - 1;
    let doe: i64 = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days_since_epoch: i64 = era * 146097 + doe - 719468;
    let secs = days_since_epoch * 86400 + (h as i64) * 3600 + (mi as i64) * 60 + se as i64;
    (secs as f64) * 1000.0 + ms as f64
}

/// Convert ms since epoch back to (Y, M, D, h, mi, se, ms).
fn ms_to_date(ms: f64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let secs = (ms / 1000.0).floor() as i64;
    let mut days = secs.div_euclid(86400);
    let mut rem = secs.rem_euclid(86400) as u32;
    let h = rem / 3600;
    rem %= 3600;
    let mi = rem / 60;
    let se = rem % 60;
    let ms_part = (ms - (secs as f64) * 1000.0).round() as u32;
    days += 719468;
    let era = if days >= 0 { days / 146097 } else { (days - 146096) / 146097 };
    let doe: i64 = days - era * 146097;
    let yoe: i64 = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y0: i64 = yoe + era * 400;
    let doy: i64 = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp: i64 = (5 * doy + 2) / 153;
    let d_part: u32 = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m_part: u32 = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let y_part: i32 = if m_part <= 2 { (y0 + 1) as i32 } else { y0 as i32 };
    (y_part, m_part, d_part, h, mi, se, ms_part)
}

/// Parse a duration like `5d`, `30d`, `24h`, `0.5w`, `30ms`.
fn parse_duration(s: &str) -> Option<f64> {
    let s = s.trim();
    // /^(\d+(?:\.\d+)?)([Mdhmswy]|ms)$/
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let num_part = &s[..i];
    let unit_part = &s[i..];
    let value: f64 = num_part.parse().ok()?;
    let unit_ms = match unit_part {
        "ms" => 1.0,
        "s" => 1000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        "d" => 86_400_000.0,
        "w" => 7.0 * 86_400_000.0,
        "M" => 30.0 * 86_400_000.0, // approximate; real dayjs handles months calendrically
        "y" => 365.0 * 86_400_000.0,
        _ => return None,
    };
    Some(value * unit_ms)
}

// ── Task resolution ──────────────────────────────────────────────────

fn resolve_tasks(d: &GanttDiagram) -> Vec<ResolvedTask> {
    use std::collections::HashMap;

    let date_format = if d.date_format.trim().is_empty() {
        "YYYY-MM-DD".to_string()
    } else {
        d.date_format.clone()
    };

    let mut resolved: Vec<ResolvedTask> = Vec::with_capacity(d.tasks.len());
    let mut id_to_idx: HashMap<String, usize> = HashMap::new();

    let mut prev_end: Option<f64> = None;

    for (i, t) in d.tasks.iter().enumerate() {
        let id = t.id.clone().unwrap_or_else(|| format!("task{}", i + 1));
        let section_name = d
            .sections
            .get(t.section)
            .map(|s| s.name.clone())
            .unwrap_or_default();

        let (start_ms, end_ms, manual_end) = compute_task_times(
            t,
            &date_format,
            prev_end,
            &id_to_idx,
            &resolved,
        );

        let (final_end, render_end) = if !d.excludes.is_empty() && !manual_end {
            apply_exclude_dates(start_ms, end_ms, &date_format, d)
        } else {
            (end_ms, None)
        };

        let task = ResolvedTask {
            id: id.clone(),
            name: t.name.clone(),
            start_ms,
            end_ms: final_end,
            render_end_ms: render_end,
            done: t.done,
            active: t.active,
            critical: t.critical,
            milestone: t.milestone,
            vert: false,
            section_idx: t.section,
            section_name,
            classes: t.classes.clone(),
            order: i,
        };
        prev_end = Some(final_end);
        id_to_idx.insert(id, resolved.len());
        resolved.push(task);
    }

    resolved
}

fn compute_task_times(
    t: &Task,
    date_format: &str,
    prev_end: Option<f64>,
    id_to_idx: &std::collections::HashMap<String, usize>,
    resolved: &[ResolvedTask],
) -> (f64, f64, bool) {
    let start_ms = if let Some(start_str) = t.start.as_deref() {
        get_start_date(start_str, date_format, id_to_idx, resolved).unwrap_or(0.0)
    } else {
        prev_end.unwrap_or(0.0)
    };
    let manual_end = t
        .end
        .as_deref()
        .map(|e| parse_iso_date(e.trim()).is_some())
        .unwrap_or(false);
    let end_ms = if let Some(end_str) = t.end.as_deref() {
        get_end_date(end_str, date_format, start_ms, id_to_idx, resolved).unwrap_or(start_ms)
    } else {
        start_ms
    };
    (start_ms, end_ms, manual_end)
}

fn get_start_date(
    s: &str,
    date_format: &str,
    id_to_idx: &std::collections::HashMap<String, usize>,
    resolved: &[ResolvedTask],
) -> Option<f64> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("after ") {
        let mut latest: Option<f64> = None;
        for id in rest.split_whitespace() {
            if let Some(&idx) = id_to_idx.get(id) {
                let end = resolved[idx].end_ms;
                if latest.map_or(true, |cur| end > cur) {
                    latest = Some(end);
                }
            }
        }
        return latest;
    }
    parse_date(s, date_format)
}

fn get_end_date(
    s: &str,
    date_format: &str,
    start_ms: f64,
    id_to_idx: &std::collections::HashMap<String, usize>,
    resolved: &[ResolvedTask],
) -> Option<f64> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("until ") {
        let mut earliest: Option<f64> = None;
        for id in rest.split_whitespace() {
            if let Some(&idx) = id_to_idx.get(id) {
                let st = resolved[idx].start_ms;
                if earliest.map_or(true, |cur| st < cur) {
                    earliest = Some(st);
                }
            }
        }
        return earliest;
    }
    if let Some(d) = parse_date(s, date_format) {
        return Some(d);
    }
    if let Some(dur) = parse_duration(s) {
        return Some(start_ms + dur);
    }
    None
}

// ── Tick generation ──────────────────────────────────────────────────

fn generate_ticks(min_ms: f64, max_ms: f64, axis_format: &str, _date_format: &str) -> Vec<AxisTick> {
    if !min_ms.is_finite() || !max_ms.is_finite() || max_ms <= min_ms {
        return Vec::new();
    }
    let span = max_ms - min_ms;
    let target: f64 = 10.0;
    let target_step = span / target;

    // d3-time-scale tick interval table (ms step + interval kind).
    // Source: d3-time-scale `tickIntervals`. Each row: (step_ms,
    // interval_id) where interval_id is one of:
    //   "ms", "s", "m", "h", "d", "w", "M", "y"
    // and the multiplier baked into step_ms.
    let candidates: &[(f64, &str, u32)] = &[
        (1.0, "ms", 1),
        (5.0, "ms", 5),
        (25.0, "ms", 25),
        (50.0, "ms", 50),
        (100.0, "ms", 100),
        (250.0, "ms", 250),
        (500.0, "ms", 500),
        (1_000.0, "s", 1),
        (5_000.0, "s", 5),
        (15_000.0, "s", 15),
        (30_000.0, "s", 30),
        (60_000.0, "m", 1),
        (60_000.0 * 5.0, "m", 5),
        (60_000.0 * 15.0, "m", 15),
        (60_000.0 * 30.0, "m", 30),
        (3_600_000.0, "h", 1),
        (3_600_000.0 * 3.0, "h", 3),
        (3_600_000.0 * 6.0, "h", 6),
        (3_600_000.0 * 12.0, "h", 12),
        (86_400_000.0, "d", 1),
        (86_400_000.0 * 2.0, "d", 2),
        (86_400_000.0 * 7.0, "w", 1),
        (86_400_000.0 * 30.0, "M", 1),
        (86_400_000.0 * 90.0, "M", 3),
        (86_400_000.0 * 365.0, "y", 1),
    ];

    // Bisect right: find first index where candidate.step > target_step.
    let mut idx = candidates.len();
    for (i, (step, _, _)) in candidates.iter().enumerate() {
        if *step > target_step {
            idx = i;
            break;
        }
    }
    // Pick whichever of [idx-1] or [idx] is closer to target_step
    // (geometric mean rule: idx if target_step / step[idx-1] >
    // step[idx] / target_step).
    let chosen = if idx == 0 {
        0
    } else if idx >= candidates.len() {
        candidates.len() - 1
    } else {
        let prev = candidates[idx - 1].0;
        let cur = candidates[idx].0;
        if target_step / prev >= cur / target_step {
            idx
        } else {
            idx - 1
        }
    };
    let (best_step, kind, _mult) = candidates[chosen];

    // Generate ticks aligned to interval boundary.
    let mut ticks: Vec<AxisTick> = Vec::new();
    let aligned_starts = match kind {
        "ms" => {
            let n = (min_ms / best_step).ceil();
            let mut t = n * best_step;
            let mut v = Vec::new();
            while t <= max_ms + 0.5 {
                v.push(t);
                t += best_step;
            }
            v
        }
        "s" => {
            // align to second multiples
            let n = (min_ms / best_step).ceil();
            let mut t = n * best_step;
            let mut v = Vec::new();
            while t <= max_ms + 0.5 {
                v.push(t);
                t += best_step;
            }
            v
        }
        "m" | "h" => {
            let n = (min_ms / best_step).ceil();
            let mut t = n * best_step;
            let mut v = Vec::new();
            while t <= max_ms + 0.5 {
                v.push(t);
                t += best_step;
            }
            v
        }
        "d" => {
            // Day-aligned (UTC midnight).
            let day_ms = 86_400_000.0;
            let mult_days = best_step / day_ms;
            let n = (min_ms / day_ms / mult_days).ceil();
            let mut t = n * mult_days * day_ms;
            let mut v = Vec::new();
            while t <= max_ms + 0.5 {
                v.push(t);
                t += best_step;
            }
            v
        }
        "w" => {
            // Week-aligned to Sunday (d3 default `timeWeek`).
            // 1970-01-04 (Sunday) is 3 days after epoch.
            let day_ms = 86_400_000.0;
            let sunday_anchor = 3.0 * day_ms;
            let n = ((min_ms - sunday_anchor) / best_step).ceil();
            let mut t = sunday_anchor + n * best_step;
            let mut v = Vec::new();
            while t <= max_ms + 0.5 {
                v.push(t);
                t += best_step;
            }
            v
        }
        "M" => {
            // Month aligned: walk by calendar months from min.
            let mult = (best_step / (86_400_000.0 * 30.0)).round() as u32;
            let (y0, m0, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            let mut m = m0;
            // Round up to next month if min isn't on month boundary.
            let min_floor_ms = date_to_ms(y, m, 1, 0, 0, 0, 0);
            if min_floor_ms < min_ms {
                m += mult;
                while m > 12 {
                    m -= 12;
                    y += 1;
                }
            } else if min_floor_ms > min_ms {
                // shouldn't happen
            }
            let mut v = Vec::new();
            loop {
                let t = date_to_ms(y, m, 1, 0, 0, 0, 0);
                if t > max_ms + 0.5 {
                    break;
                }
                v.push(t);
                m += mult;
                while m > 12 {
                    m -= 12;
                    y += 1;
                }
            }
            v
        }
        "y" => {
            let (y0, _, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            // round up
            if date_to_ms(y, 1, 1, 0, 0, 0, 0) < min_ms {
                y += 1;
            }
            let mut v = Vec::new();
            loop {
                let t = date_to_ms(y, 1, 1, 0, 0, 0, 0);
                if t > max_ms + 0.5 {
                    break;
                }
                v.push(t);
                y += 1;
            }
            v
        }
        _ => Vec::new(),
    };
    for t in aligned_starts {
        ticks.push(AxisTick {
            time_ms: t,
            label: format_time(t, axis_format),
        });
    }
    ticks
}

// ── Time formatting (d3 timeFormat) ──────────────────────────────────

pub fn format_time(ms: f64, fmt: &str) -> String {
    let (y, m, d, h, mi, se, msp) = ms_to_date(ms);
    let mut out = String::with_capacity(fmt.len() + 8);
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'%' && i + 1 < bytes.len() {
            let spec = bytes[i + 1];
            match spec {
                b'Y' => out.push_str(&format!("{y:04}")),
                b'y' => out.push_str(&format!("{:02}", y % 100)),
                b'm' => out.push_str(&format!("{m:02}")),
                b'd' => out.push_str(&format!("{d:02}")),
                b'e' => out.push_str(&format!("{d:2}")),
                b'H' => out.push_str(&format!("{h:02}")),
                b'M' => out.push_str(&format!("{mi:02}")),
                b'S' => out.push_str(&format!("{se:02}")),
                // d3 Ss = padded seconds-of-minute with leading space
                b's' => out.push_str(&format!("{}", se)),
                b'L' => out.push_str(&format!("{msp:03}")),
                b'%' => out.push('%'),
                _ => {
                    // Unrecognized — emit literally.
                    out.push('%');
                    out.push(spec as char);
                }
            }
            i += 2;
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

// ── Exclude ranges ───────────────────────────────────────────────────

fn compute_exclude_ranges(min_ms: f64, max_ms: f64, d: &GanttDiagram) -> Vec<ExcludeRange> {
    let date_format = if d.date_format.trim().is_empty() {
        "YYYY-MM-DD".to_string()
    } else {
        d.date_format.clone()
    };
    if !min_ms.is_finite() || !max_ms.is_finite() {
        return Vec::new();
    }
    if (max_ms - min_ms) > 5.0 * 365.0 * 86_400_000.0 {
        return Vec::new();
    }
    let mut ranges: Vec<ExcludeRange> = Vec::new();
    let mut current: Option<(f64, f64)> = None;
    let day_ms = 86_400_000.0;
    let start_day = (min_ms / day_ms).floor() * day_ms;
    let mut day = start_day;
    while day <= max_ms {
        let invalid = is_invalid_date(day, &date_format, &d.excludes, &d.includes, &d.weekend);
        if invalid {
            current = match current {
                Some((s, _)) => Some((s, day)),
                None => Some((day, day)),
            };
        } else if let Some((s, e)) = current.take() {
            let (_, m, dy, _, _, _, _) = ms_to_date(s);
            let (yy, _, _, _, _, _, _) = ms_to_date(s);
            let iso = format!("{:04}-{:02}-{:02}", yy, m, dy);
            // end-of-day for `e`.
            let end_eod = e + day_ms - 1.0;
            ranges.push(ExcludeRange {
                start_iso: iso,
                start_ms: s,
                end_ms: end_eod,
            });
        }
        day += day_ms;
    }
    if let Some((s, e)) = current.take() {
        let (yy, m, dy, _, _, _, _) = ms_to_date(s);
        let iso = format!("{:04}-{:02}-{:02}", yy, m, dy);
        let end_eod = e + day_ms - 1.0;
        ranges.push(ExcludeRange {
            start_iso: iso,
            start_ms: s,
            end_ms: end_eod,
        });
    }
    ranges
}

fn is_invalid_date(
    day_ms: f64,
    date_format: &str,
    excludes: &[String],
    includes: &[String],
    weekend: &str,
) -> bool {
    let (y, m, d, _, _, _, _) = ms_to_date(day_ms);
    let formatted = format_time(day_ms, date_format);
    let date_only = format!("{:04}-{:02}-{:02}", y, m, d);
    if includes.iter().any(|s| s == &formatted || s == &date_only) {
        return false;
    }
    let dow = day_of_week(y, m, d); // 1=Mon..7=Sun (ISO)
    let weekend_start = match weekend {
        "friday" => 5,
        "saturday" => 6,
        _ => 6,
    };
    if excludes.iter().any(|e| e == "weekends")
        && (dow == weekend_start || dow == weekend_start + 1 || (weekend_start == 6 && dow == 7))
    {
        return true;
    }
    let day_name = match dow {
        1 => "monday",
        2 => "tuesday",
        3 => "wednesday",
        4 => "thursday",
        5 => "friday",
        6 => "saturday",
        7 => "sunday",
        _ => "",
    };
    if excludes.iter().any(|e| e == day_name) {
        return true;
    }
    if excludes.iter().any(|e| e == &formatted || e == &date_only) {
        return true;
    }
    false
}

fn day_of_week(y: i32, m: u32, d: u32) -> u32 {
    // Zeller's congruence for ISO weekday (1=Mon..7=Sun).
    let (yy, mm) = if m < 3 { (y - 1, m + 12) } else { (y, m) };
    let k = (yy % 100 + 400) % 100;
    let j = ((yy as i64) / 100).rem_euclid(400) as i32;
    let h = (d as i32 + (13 * ((mm + 1) as i32)) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7);
    // Zeller h: 0=Sat,1=Sun,2=Mon,...
    // Convert to ISO: 1=Mon..7=Sun.
    match h {
        0 => 6, // Sat
        1 => 7, // Sun
        n => (n - 1) as u32,
    }
}

fn apply_exclude_dates(
    start_ms: f64,
    end_ms: f64,
    date_format: &str,
    d: &GanttDiagram,
) -> (f64, Option<f64>) {
    // checkTaskDates: starts at start+1d, walks to end, on each invalid
    // day pushes end forward by 1 day; renderEndTime is the original
    // end (unless start > end).
    let day_ms = 86_400_000.0;
    let mut s = start_ms + day_ms;
    let mut e = end_ms;
    let mut render_end: Option<f64> = None;
    let mut invalid = false;
    while s <= e {
        if !invalid {
            render_end = Some(e);
        }
        invalid = is_invalid_date(s, date_format, &d.excludes, &d.includes, &d.weekend);
        if invalid {
            e += day_ms;
        }
        s += day_ms;
    }
    (e, render_end)
}
