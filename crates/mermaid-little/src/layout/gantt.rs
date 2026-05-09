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
    /// Raw start (inherits minTime's time-of-day).
    pub raw_start_ms: f64,
    /// Raw end of the run (last invalid day at same time-of-day).
    pub raw_end_ms: f64,
    /// `start.startOf('day')` — used for `x`.
    pub start_of_day_ms: f64,
    /// `end.endOf('day')` — used for `width`.
    pub end_eod_ms: f64,
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

    // Categories use insertion order — upstream computes them BEFORE
    // sorting `taskArray.sort(taskCompare)`.
    let mut categories: Vec<String> = Vec::new();
    for t in &resolved {
        if !categories.iter().any(|c| c == &t.section_name) {
            categories.push(t.section_name.clone());
        }
    }

    // Sort by start time (stable) for rendering — upstream calls
    // `taskArray.sort(taskCompare)` which only compares startTime.
    let mut tasks = resolved;
    tasks.sort_by(|a, b| {
        a.start_ms
            .partial_cmp(&b.start_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute category heights and total height.
    let compact = matches!(d.display_mode.as_deref(), Some(s) if s.eq_ignore_ascii_case("compact"));
    let mut category_heights: Vec<(String, i32)> = Vec::new();
    let h: i32;
    if compact {
        // displayMode: compact — pack tasks per-section into rows so that
        // non-overlapping tasks share the same vertical slot. Mirrors
        // upstream `ganttRenderer.draw()` lines 1862-1884.
        //
        // The packing uses encounter-order of sections (NOT insertion);
        // upstream iterates `Object.keys(categoryElements)` which in
        // modern JS preserves insertion order — i.e. the order of first
        // task seen per section in `taskArray` (post-sort).
        let mut section_order: Vec<String> = Vec::new();
        let mut section_task_indices: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, t) in tasks.iter().enumerate() {
            if !section_task_indices.contains_key(&t.section_name) {
                section_order.push(t.section_name.clone());
            }
            section_task_indices
                .entry(t.section_name.clone())
                .or_default()
                .push(i);
        }
        let mut intersections: i32 = 0;
        for cat in &section_order {
            let task_indices = section_task_indices.get(cat).cloned().unwrap_or_default();
            // Run getMaxIntersections to pack tasks; mutates t.order in place.
            let max_j = compact_pack(&task_indices, intersections, &mut tasks);
            let category_height = max_j + 1;
            category_heights.push((cat.clone(), category_height));
            intersections += category_height;
        }
        // Sections that exist but had zero tasks: still record height 0.
        // (Upstream's `categoryHeights` only contains sections that have
        // at least one task; preserve that behaviour.)
        let total_rows: i32 = category_heights.iter().map(|(_, h)| *h).sum();
        h = 2 * TOP_PADDING + total_rows * (BAR_HEIGHT + BAR_GAP);
    } else {
        for cat in &categories {
            let count = tasks.iter().filter(|t| &t.section_name == cat).count() as i32;
            category_heights.push((cat.clone(), count));
        }
        h = 2 * TOP_PADDING + (tasks.len() as i32) * (BAR_HEIGHT + BAR_GAP);
    }

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

    // Tick generation — honor `tickInterval` if provided, otherwise
    // fall back to d3-style closest-to-10 selection.
    let axis_ticks = if let Some(ti) = d.tick_interval.as_deref() {
        generate_ticks_fixed(min_ms, max_ms, &axis_format, ti, &d.weekday)
            .unwrap_or_else(|| generate_ticks(min_ms, max_ms, &axis_format, &d.date_format))
    } else {
        generate_ticks(min_ms, max_ms, &axis_format, &d.date_format)
    };

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

/// Pack a section's tasks into the minimum number of rows so non-overlapping
/// tasks share a row. Mirrors upstream `getMaxIntersections`. Returns the
/// largest `j` (0-based row index) used; the section therefore consumes
/// `j+1` rows. As a side-effect, `tasks[idx].order` is set to `j + offset`
/// for each task in `task_indices`.
fn compact_pack(task_indices: &[usize], offset: i32, tasks: &mut [ResolvedTask]) -> i32 {
    if task_indices.is_empty() {
        return 0;
    }
    // Capture (start, original_order, idx) so we can sort while keeping
    // a back-reference to the task we mutate.
    let mut items: Vec<(f64, usize, usize)> = task_indices
        .iter()
        .map(|&i| (tasks[i].start_ms, tasks[i].order, i))
        .collect();
    // Sort by (startTime, original_order) — matches upstream's
    // `(a,b) => a.startTime - b.startTime || a.order - b.order`.
    items.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    // Timeline of `endTime` per row, length = number of tasks (upper bound).
    let mut timeline: Vec<f64> = vec![f64::NEG_INFINITY; items.len()];
    let mut max_j = 0i32;
    for (start, _orig_order, idx) in items {
        for (j, slot) in timeline.iter_mut().enumerate() {
            if start >= *slot {
                *slot = tasks[idx].end_ms;
                tasks[idx].order = (j as i32 + offset) as usize;
                if j as i32 > max_j {
                    max_j = j as i32;
                }
                break;
            }
        }
    }
    max_j
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
    // Strict mode failure falls through to the lenient `new Date(str)`
    // path that upstream uses as fallback.
    if fmt == "YYYY-MM-DD" {
        if let Some(t) = parse_iso_date(s) {
            return Some(t);
        }
        return parse_iso_date_lenient(s);
    }

    // YYYY-MM-DD with time component(s).
    if fmt.starts_with("YYYY-MM-DD ") || fmt.starts_with("YYYY-MM-DDT") {
        // Trim everything after the date portion.
        if s.len() < 10 {
            return None;
        }
        let date = parse_iso_date(&s[..10])?;
        // Parse trailing time HH:mm:ss / HH:mm if present.
        let (h, mi, se) = parse_time_suffix(&s[10..]);
        return Some(
            date + (h as f64) * 3_600_000.0 + (mi as f64) * 60_000.0 + (se as f64) * 1000.0,
        );
    }

    // Time-only formats. dayjs anchors them on "today" but for byte-exact
    // rendering we only need consistent relative offsets — anchor on
    // 1970-01-01 (epoch midnight) instead. The chart shows hours/minutes
    // anyway, never the year.
    if fmt == "HH:mm:ss" || fmt == "HH:mm" || fmt == "H:mm:ss" || fmt == "H:mm" {
        let parts: Vec<&str> = s.split(':').collect();
        let h: u32 = parts.first().and_then(|x| x.parse().ok())?;
        let mi: u32 = parts.get(1).and_then(|x| x.parse().ok())?;
        let se: u32 = parts.get(2).and_then(|x| x.parse().ok()).unwrap_or(0);
        return Some((h as f64) * 3_600_000.0 + (mi as f64) * 60_000.0 + (se as f64) * 1000.0);
    }

    // Some common compounds with time component.
    if let Some(t) = parse_iso_date(s) {
        return Some(t);
    }
    // Final fallback mirrors upstream's `new Date(str)` path — try a
    // lenient ISO parse so things like "2019-09-1" still work.
    parse_iso_date_lenient(s)
}

fn parse_time_suffix(s: &str) -> (u32, u32, u32) {
    // "T20:30:30" or " 20:30:30" or " 20:30" — return 0,0,0 if any failure.
    let s = s.trim_start_matches(|c: char| c == 'T' || c.is_whitespace());
    let parts: Vec<&str> = s.split(':').collect();
    let h = parts
        .first()
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or(0);
    let mi = parts
        .get(1)
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or(0);
    let se = parts
        .get(2)
        .and_then(|x| x.parse::<u32>().ok())
        .unwrap_or(0);
    (h, mi, se)
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

/// Lenient ISO parse — accepts `YYYY-MM-D` or `YYYY-M-DD` etc. used as
/// `new Date(str)` fallback in upstream when strict parsing fails.
fn parse_iso_date_lenient(s: &str) -> Option<f64> {
    if let Some(v) = parse_iso_date(s) {
        return Some(v);
    }
    let parts: Vec<&str> = s.splitn(3, '-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    // Day part may have trailing time-of-day separated by `T` or space.
    let d_str = parts[2];
    let d_only: &str = d_str
        .find(|c: char| c == 'T' || c == ' ')
        .map(|i| &d_str[..i])
        .unwrap_or(d_str);
    let d: u32 = d_only.parse().ok()?;
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
    let era = if days >= 0 {
        days / 146097
    } else {
        (days - 146096) / 146097
    };
    let doe: i64 = days - era * 146097;
    let yoe: i64 = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y0: i64 = yoe + era * 400;
    let doy: i64 = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp: i64 = (5 * doy + 2) / 153;
    let d_part: u32 = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m_part: u32 = if mp < 10 {
        (mp + 3) as u32
    } else {
        (mp - 9) as u32
    };
    let y_part: i32 = if m_part <= 2 {
        (y0 + 1) as i32
    } else {
        y0 as i32
    };
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
            d.inclusive_end_dates,
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
            vert: t.vert,
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
    inclusive: bool,
) -> (f64, f64, bool) {
    let start_ms = if let Some(start_str) = t.start.as_deref() {
        get_start_date(start_str, date_format, id_to_idx, resolved).unwrap_or(0.0)
    } else {
        prev_end.unwrap_or(0.0)
    };
    // Upstream: `dayjs(endTimeData, 'YYYY-MM-DD', true).isValid()`
    // — strict mode, so the entire string must match exactly.
    let manual_end = t
        .end
        .as_deref()
        .map(|e| {
            let s = e.trim();
            s.len() == 10 && parse_iso_date(s).is_some()
        })
        .unwrap_or(false);
    let end_ms = if let Some(end_str) = t.end.as_deref() {
        get_end_date(
            end_str,
            date_format,
            start_ms,
            id_to_idx,
            resolved,
            inclusive,
        )
        .unwrap_or(start_ms)
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
    inclusive: bool,
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
        // Inclusive end-dates: when the end was a parsed date, add 1d.
        return Some(if inclusive { d + 86_400_000.0 } else { d });
    }
    if let Some(dur) = parse_duration(s) {
        return Some(start_ms + dur);
    }
    None
}

// ── Tick generation ──────────────────────────────────────────────────

/// Honour an explicit `tickInterval` directive.
/// Pattern: `^([1-9]\d*)(millisecond|second|minute|hour|day|week|month)$`.
fn generate_ticks_fixed(
    min_ms: f64,
    max_ms: f64,
    axis_format: &str,
    tick_interval: &str,
    weekday: &str,
) -> Option<Vec<AxisTick>> {
    let tick_interval = tick_interval.trim();
    let bytes = tick_interval.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let n: u32 = tick_interval[..i].parse().ok()?;
    if n == 0 {
        return None;
    }
    let unit = &tick_interval[i..];

    // MAX_TICK_COUNT 10000 check.
    let unit_ms = match unit {
        "millisecond" => 1.0,
        "second" => 1000.0,
        "minute" => 60_000.0,
        "hour" => 3_600_000.0,
        "day" => 86_400_000.0,
        "week" => 7.0 * 86_400_000.0,
        "month" => 30.0 * 86_400_000.0,
        _ => return None,
    };
    let estimated = ((max_ms - min_ms) / (unit_ms * n as f64)).ceil();
    if estimated > 10_000.0 {
        return None;
    }

    let mut v: Vec<f64> = Vec::new();
    match unit {
        "millisecond" | "second" | "minute" => {
            let step = unit_ms * n as f64;
            let s = (min_ms / step).ceil() * step;
            let mut t = s;
            while t <= max_ms + 0.5 {
                v.push(t);
                t += step;
            }
        }
        "hour" => {
            // d3 timeHour.every(k) anchors hour-of-day to multiples of k from 0.
            let day_ms = 86_400_000.0;
            let hour_ms = 3_600_000.0;
            // Find first hour-aligned tick at hour H where H % n == 0 and t >= min.
            let day_start = (min_ms / day_ms).floor() * day_ms;
            let mut t = day_start;
            // advance to first hour multiple in/after min.
            while t < min_ms {
                t += hour_ms * n as f64;
            }
            // ensure alignment to multiple of n hours from day_start.
            let hours_from_day = ((t - day_start) / hour_ms).round() as i32;
            let aligned = (hours_from_day / n as i32) * n as i32;
            t = day_start + (aligned as f64) * hour_ms;
            while t < min_ms {
                t += hour_ms * n as f64;
            }
            while t <= max_ms + 0.5 {
                v.push(t);
                t += hour_ms * n as f64;
            }
        }
        "day" => {
            // Already handled by general d-branch; replicate here for fixed.
            let day_ms = 86_400_000.0;
            let (y0, m0, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            let mut m = m0;
            let mut done = false;
            while !done && y <= 9999 {
                let dim = days_in_month(y, m);
                let mut anchor_day: u32 = 1;
                while anchor_day <= dim {
                    let t = date_to_ms(y, m, anchor_day, 0, 0, 0, 0);
                    if t > max_ms + 0.5 {
                        done = true;
                        break;
                    }
                    if t >= min_ms {
                        v.push(t);
                    }
                    anchor_day += n;
                }
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
            let _ = day_ms;
        }
        "week" => {
            // Week aligned to `weekday` config (default sunday).
            let day_ms = 86_400_000.0;
            let week_step = 7.0 * day_ms * n as f64;
            // anchor: nearest weekday start before/at min.
            let target_dow_iso = match weekday {
                "monday" => 1,
                "tuesday" => 2,
                "wednesday" => 3,
                "thursday" => 4,
                "friday" => 5,
                "saturday" => 6,
                _ => 7, // sunday
            };
            // 1970-01-01 was Thursday=4 ISO. Compute offset.
            let anchor_ms = anchor_for_iso_weekday(target_dow_iso);
            let n_iter = ((min_ms - anchor_ms) / week_step).ceil();
            let mut t = anchor_ms + n_iter * week_step;
            while t <= max_ms + 0.5 {
                v.push(t);
                t += week_step;
            }
        }
        "month" => {
            let mult = n;
            let (y0, m0, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            let mut m = m0;
            // round up to next month if min not on month boundary.
            let min_floor = date_to_ms(y, m, 1, 0, 0, 0, 0);
            if min_floor < min_ms {
                m += mult;
                while m > 12 {
                    m -= 12;
                    y += 1;
                }
            }
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
                if y > 9999 {
                    break;
                }
            }
        }
        _ => return None,
    }

    let ticks: Vec<AxisTick> = v
        .into_iter()
        .map(|t| AxisTick {
            time_ms: t,
            label: format_time(t, axis_format),
        })
        .collect();
    Some(ticks)
}

fn anchor_for_iso_weekday(target_dow: u32) -> f64 {
    // Find a known timestamp where weekday = target_dow.
    // 1970-01-04 (Sunday=7), 1970-01-05 (Monday=1), 1970-01-06 (Tuesday=2),
    // 1970-01-07 (Wednesday=3), 1970-01-01 (Thursday=4),
    // 1970-01-02 (Friday=5), 1970-01-03 (Saturday=6).
    let day_ms = 86_400_000.0;
    let offset_days = match target_dow {
        1 => 4, // Monday
        2 => 5, // Tuesday
        3 => 6, // Wednesday
        4 => 0, // Thursday (epoch itself)
        5 => 1, // Friday
        6 => 2, // Saturday
        7 => 3, // Sunday
        _ => 3,
    };
    (offset_days as f64) * day_ms
}

/// Mimic d3-array `tickIncrement(start, stop, count)` — returns the
/// integer step used in `i1, i2, inc`. Positive meaning multiply, negative
/// meaning divide; we always return the absolute "multiply" form for
/// year-step usage (1, 2, 5, 10, 20, 50, 100, ...).
fn d3_tick_step_year(start: f64, stop: f64, count: f64) -> u32 {
    let step = (stop - start) / count.max(0.0);
    if step <= 0.0 || !step.is_finite() {
        return 1;
    }
    let power = step.log10().floor();
    // d3-array uses these specific floating-point thresholds (matching
    // sqrt(50), sqrt(10), sqrt(2)). Clippy's "use std::f64::consts" hint
    // doesn't apply since the *exact* d3 source uses these literals.
    #[allow(clippy::approx_constant)]
    let e10 = 7.0710678118654755_f64;
    #[allow(clippy::approx_constant)]
    let e5 = 3.1622776601683795_f64;
    #[allow(clippy::approx_constant)]
    let e2 = 1.4142135623730951_f64;
    let error = step / 10f64.powf(power);
    let factor: f64 = if error >= e10 {
        10.0
    } else if error >= e5 {
        5.0
    } else if error >= e2 {
        2.0
    } else {
        1.0
    };
    let inc = 10f64.powf(power) * factor;
    let inc = if inc.is_finite() && inc >= 1.0 {
        inc.round() as u32
    } else {
        1
    };
    inc.max(1)
}

fn generate_ticks(
    min_ms: f64,
    max_ms: f64,
    axis_format: &str,
    _date_format: &str,
) -> Vec<AxisTick> {
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
    // d3's path for span much larger than 1 year: derive a multi-year
    // step via tickStep(start/y, stop/y, count). This corresponds to
    // `if (i === tickIntervals.length) return year.every(tickStep(...))`.
    let year_ms = 365.25 * 86_400_000.0; // d3 durationYear
    let multi_year_step: u32 = if idx == candidates.len() {
        d3_tick_step_year(min_ms / year_ms, max_ms / year_ms, target)
    } else {
        1
    };
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
            // d3 `timeDay.every(k)` anchors on day-of-month (1, k+1,
            // 2k+1, …), resetting at each month boundary.
            let day_ms = 86_400_000.0;
            let k = (best_step / day_ms).round().max(1.0) as u32;
            let mut v = Vec::new();
            let (y0, m0, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            let mut m = m0;
            let mut done = false;
            while !done && y <= 9999 {
                let dim = days_in_month(y, m);
                let mut anchor_day: u32 = 1;
                while anchor_day <= dim {
                    let t = date_to_ms(y, m, anchor_day, 0, 0, 0, 0);
                    if t > max_ms + 0.5 {
                        done = true;
                        break;
                    }
                    if t >= min_ms {
                        v.push(t);
                    }
                    anchor_day += k;
                }
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
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
            // d3's `year.every(N)` filters to years that are integer
            // multiples of N (anchored at year 0). For N>1 this means
            // ticks at 1000, 2000, 3000, … (or 5, 10, 100 etc).
            let step = multi_year_step.max(1) as i32;
            let (y0, _, _, _, _, _, _) = ms_to_date(min_ms);
            let mut y = y0;
            // Align up to next multiple of step.
            let rem = y.rem_euclid(step);
            if rem != 0 {
                y += step - rem;
            }
            // If aligned start is still < min_ms, advance another step.
            while date_to_ms(y, 1, 1, 0, 0, 0, 0) < min_ms {
                y += step;
            }
            let mut v = Vec::new();
            loop {
                let t = date_to_ms(y, 1, 1, 0, 0, 0, 0);
                if t > max_ms + 0.5 {
                    break;
                }
                v.push(t);
                y += step;
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
                b'a' => {
                    // Short weekday name (Sun, Mon, …)
                    let dow = day_of_week(y, m, d); // 1..=7 ISO
                    let name = match dow {
                        1 => "Mon",
                        2 => "Tue",
                        3 => "Wed",
                        4 => "Thu",
                        5 => "Fri",
                        6 => "Sat",
                        7 => "Sun",
                        _ => "",
                    };
                    out.push_str(name);
                }
                b'A' => {
                    let dow = day_of_week(y, m, d);
                    let name = match dow {
                        1 => "Monday",
                        2 => "Tuesday",
                        3 => "Wednesday",
                        4 => "Thursday",
                        5 => "Friday",
                        6 => "Saturday",
                        7 => "Sunday",
                        _ => "",
                    };
                    out.push_str(name);
                }
                b'b' => {
                    let name = match m {
                        1 => "Jan",
                        2 => "Feb",
                        3 => "Mar",
                        4 => "Apr",
                        5 => "May",
                        6 => "Jun",
                        7 => "Jul",
                        8 => "Aug",
                        9 => "Sep",
                        10 => "Oct",
                        11 => "Nov",
                        12 => "Dec",
                        _ => "",
                    };
                    out.push_str(name);
                }
                b'B' => {
                    let name = match m {
                        1 => "January",
                        2 => "February",
                        3 => "March",
                        4 => "April",
                        5 => "May",
                        6 => "June",
                        7 => "July",
                        8 => "August",
                        9 => "September",
                        10 => "October",
                        11 => "November",
                        12 => "December",
                        _ => "",
                    };
                    out.push_str(name);
                }
                b'j' => {
                    // Day of year (001-366)
                    let mut doy = 0u32;
                    for mm in 1..m {
                        doy += days_in_month(y, mm);
                    }
                    doy += d;
                    out.push_str(&format!("{doy:03}"));
                }
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
    // Upstream starts at `dayjs(minTime)` (== minTime as-is, including
    // its time-of-day) and adds 1 day per step. The range start/end
    // therefore inherit minTime's time-of-day; the `x`/`width`
    // computations use `startOf('day')`/`endOf('day')` which
    // re-anchor to midnight.
    let mut day = min_ms;
    while day <= max_ms {
        let invalid = is_invalid_date(day, &date_format, &d.excludes, &d.includes, &d.weekend);
        if invalid {
            current = match current {
                Some((s, _)) => Some((s, day)),
                None => Some((day, day)),
            };
        } else if let Some((s, e)) = current.take() {
            ranges.push(make_exclude_range(s, e, day_ms));
        }
        day += day_ms;
    }
    if let Some((s, e)) = current.take() {
        ranges.push(make_exclude_range(s, e, day_ms));
    }
    ranges
}

fn make_exclude_range(s: f64, e: f64, day_ms: f64) -> ExcludeRange {
    let (yy, m, dy, _, _, _, _) = ms_to_date(s);
    let iso = format!("{:04}-{:02}-{:02}", yy, m, dy);
    let start_of_day_s = floor_to_day(s);
    let end_of_day_e = floor_to_day(e) + day_ms - 1.0;
    ExcludeRange {
        start_iso: iso,
        raw_start_ms: s,
        raw_end_ms: e,
        start_of_day_ms: start_of_day_s,
        end_eod_ms: end_of_day_e,
    }
}

fn floor_to_day(ms: f64) -> f64 {
    let day_ms = 86_400_000.0;
    (ms / day_ms).floor() * day_ms
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

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0);
            if leap {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn ticks_from(times: Vec<f64>, axis_format: &str) -> Vec<f64> {
    // Helper kept for symmetry — actual conversion happens in
    // `generate_ticks` after the match. We just return times here.
    let _ = axis_format;
    times
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
