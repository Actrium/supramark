//! Journey (user-journey) layout.
//!
//! Upstream reference: `packages/mermaid/src/diagrams/user-journey/journeyRenderer.ts`
//! plus `svgDraw.js`.
//!
//! The layout is a fixed grid:
//!
//! * The actor legend sits top-left, circle at `(20, yPos)` starting
//!   at `yPos = 60`. Each circle is followed by 1+ text lines for the
//!   actor name, word-wrapped to `conf.maxLabelWidth` pixels.
//! * `leftMargin = conf.leftMargin + maxWidth` where `maxWidth` is
//!   the widest wrapped legend line. Section/task grid starts at that
//!   x (upstream calls this the "leftMargin" after legend).
//! * Tasks run left-to-right. Task `i` has `x = i*(conf.width +
//!   conf.taskMargin) + leftMargin`.
//! * Sections are drawn as rectangles spanning their contiguous tasks
//!   at y=50 (section header), task rectangles at y=taskPos
//!   (= verticalPos + conf.height*2 + conf.diagramMarginY = 0 + 65*2 +
//!   10 = 140… but actually conf.height=50 in the schema used, so
//!   taskPos = 0 + 50*2 + 10 = 110 ✓).
//! * Activity arrow line at `y = conf.height * 4 = 200`.
//! * Total height (before title extra): `stopy - starty + 2*diagramMarginY`
//!   where `stopy = 300 + 5*30 = 450` from the bounds inserts, so
//!   `height = 450 - (-boxMargin*K) + 2*10 = 520 + extra`.
//!
//! Measurement notes (Wave-1 lesson 5): text nodes get their font from
//! the `<style>` block; jsdom does not apply stylesheets, so
//! `getBoundingClientRect` falls back to 14px sans-serif.

use crate::error::Result;
use crate::font_metrics::text_width;
use crate::model::journey::JourneyDiagram;
use crate::theme::ThemeVariables;

/// jsdom fallback font for all `<text>` nodes whose font-family comes
/// from CSS — 14px "sans-serif".
const JSDOM_FALLBACK_FAMILY: &str = "sans-serif";
const JSDOM_FALLBACK_SIZE: f64 = 14.0;

#[derive(Debug, Clone)]
pub struct JourneyActorLegendLine {
    pub text: String,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct JourneyActorLegend {
    /// Index among actors (for the `actor-<n>` CSS class).
    pub pos: usize,
    pub name: String,
    pub colour: String,
    pub circle_cy: f64,
    pub lines: Vec<JourneyActorLegendLine>,
}

#[derive(Debug, Clone)]
pub struct JourneyTaskLayout {
    pub index: usize,
    pub x: f64,
    pub y: f64,
    /// Section fill color (the section colour wraps around the 7-color palette).
    pub fill: String,
    /// Section numeric type (modular within the palette).
    pub num: usize,
    /// Task description (the label).
    pub task: String,
    pub score: Option<f64>,
    /// Center of the face glyph.
    pub face_cy: f64,
    /// Which face glyph to draw: 'smile' / 'sad' / 'ambivalent' / none.
    pub face: JourneyFace,
    /// Actors to draw on the top edge of the task rectangle, in input order.
    pub actor_circles: Vec<JourneyActorChip>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JourneyFace {
    Smile,
    Sad,
    Ambivalent,
    None,
}

#[derive(Debug, Clone)]
pub struct JourneyActorChip {
    pub pos: usize,
    pub cx: f64,
    pub colour: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct JourneySectionLayout {
    pub x: f64,
    pub width: f64,
    pub fill: String,
    pub num: usize,
    pub text: String,
    pub colour: String,
    /// Number of tasks this section spans (for width calc).
    pub task_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct JourneyLayout {
    /// Total SVG width (used for `max-width:` and viewBox width).
    pub width: f64,
    /// Total SVG height (before extra title height).
    pub height: f64,
    /// Extra vertical added for title (0 or 70).
    pub title_extra: f64,
    /// leftMargin after the legend column (x-origin of sections/tasks).
    pub left_margin: f64,
    pub has_title: bool,
    /// Title text (no escaping) — renderer escapes.
    pub title: String,
    pub title_color: String,
    pub title_font_family: String,
    pub title_font_size: String,
    pub actors: Vec<JourneyActorLegend>,
    pub sections: Vec<JourneySectionLayout>,
    pub tasks: Vec<JourneyTaskLayout>,
    pub task_font_family: String,
    pub task_font_size: i64,
    /// Baseline arrow line endpoints: (x1, x2, y).
    pub arrow_x1: f64,
    pub arrow_x2: f64,
    pub arrow_y: f64,
}

pub fn layout(d: &JourneyDiagram, _theme: &ThemeVariables) -> Result<JourneyLayout> {
    let conf = &d.config;

    // ── Actor legend ─────────────────────────────────────────────
    // Upstream iterates object keys (insertion order). `journeyDb.js`
    // `getActors` returns the set sorted alphabetically. Our parser
    // keeps input order on `people`; downstream actors for the legend
    // are the unique union sorted lexicographically.
    let mut actor_names: Vec<String> = Vec::new();
    for t in &d.tasks {
        for p in &t.people {
            if !actor_names.contains(p) {
                actor_names.push(p.clone());
            }
        }
    }
    actor_names.sort();

    // Build actor → pos (and colour) map by input order.
    // Actually the `actorPos++` assignment happens in iteration of the
    // sorted list — so pos is the index in the sorted list.
    let actor_pos: Vec<(String, usize, String)> = actor_names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let colour = conf.actor_colours[i % conf.actor_colours.len()].clone();
            (n.clone(), i, colour)
        })
        .collect();

    // Legend-row y & text wrap.
    let mut legend = Vec::with_capacity(actor_pos.len());
    let mut y_pos = 60.0_f64;
    let mut max_legend_w: f64 = 0.0;
    for (name, pos, colour) in &actor_pos {
        let lines = wrap_text(name, conf.max_label_width);
        let mut out_lines = Vec::with_capacity(lines.len());
        for (index, line) in lines.iter().enumerate() {
            let ly = y_pos + 7.0 + (index as f64) * 20.0;
            // Measure rendered width for maxWidth expansion. Upstream:
            //   if (lineWidth > maxWidth && lineWidth > conf.leftMargin - lineWidth)
            //     maxWidth = lineWidth;
            let w = jsdom_width(line);
            if w > max_legend_w && w > conf.left_margin - w {
                max_legend_w = w;
            }
            out_lines.push(JourneyActorLegendLine {
                text: line.clone(),
                y: ly,
            });
        }
        legend.push(JourneyActorLegend {
            pos: *pos,
            name: name.clone(),
            colour: colour.clone(),
            circle_cy: y_pos,
            lines: out_lines,
        });
        // yPos += Math.max(20, lines.length * 20);
        y_pos += (20.0_f64).max(lines.len() as f64 * 20.0);
    }

    let left_margin = conf.left_margin + max_legend_w;

    // ── Tasks & sections ─────────────────────────────────────────
    // Upstream `drawTasks`:
    //   sectionVHeight = conf.height * 2 + conf.diagramMarginY
    //   taskPos = verticalPos + sectionVHeight
    //   verticalPos = 0 initially. Using the schema's default
    //   height=50, diagramMarginY=10, taskPos = 110.
    let task_y = conf.height * 2.0 + conf.diagram_margin_y;

    // Iterate tasks; group consecutive same-section tasks into sections.
    let mut sections: Vec<JourneySectionLayout> = Vec::new();
    let mut tasks: Vec<JourneyTaskLayout> = Vec::with_capacity(d.tasks.len());

    let mut section_number: usize = 0;
    let mut last_section: Option<String> = None;
    let mut current_fill = String::from("#CCC");
    let mut current_num: usize = 0;

    for (i, t) in d.tasks.iter().enumerate() {
        let task_x = (i as f64) * conf.task_margin + (i as f64) * conf.width + left_margin;
        let new_section = match &last_section {
            None => true,
            Some(s) => s != &t.section,
        };
        if new_section {
            let num = section_number % conf.section_fills.len();
            current_fill = conf.section_fills[num].clone();
            let current_colour =
                conf.section_colours[section_number % conf.section_colours.len()].clone();
            current_num = num;

            // count consecutive same-section tasks starting at i.
            let mut count = 0usize;
            for t2 in d.tasks.iter().skip(i) {
                if t2.section == t.section {
                    count += 1;
                } else {
                    break;
                }
            }
            sections.push(JourneySectionLayout {
                x: task_x,
                width: conf.width * count as f64 + conf.diagram_margin_x * (count as f64 - 1.0),
                fill: current_fill.clone(),
                num: current_num,
                text: t.section.clone(),
                colour: current_colour.clone(),
                task_count: count,
            });
            last_section = Some(t.section.clone());
            section_number += 1;
        }

        // Face geometry per svgDraw.drawTask:
        //   cy = 300 + (5 - score) * 30  (NaN for missing score)
        //   score > 3 → smile, < 3 → sad, else ambivalent.
        let face_cy = t
            .score
            .map(|s| 300.0 + (5.0 - s) * 30.0)
            .unwrap_or(f64::NAN);
        let face = match t.score {
            Some(s) if s > 3.0 => JourneyFace::Smile,
            Some(s) if s < 3.0 => JourneyFace::Sad,
            Some(_) => JourneyFace::Ambivalent,
            None => JourneyFace::Ambivalent, // NaN comparisons are false, falls through to else
        };

        // Actor circles along top edge. Upstream loops task.people (NOT
        // the unique set) and looks up each one in the diagram-level
        // `actors` map. When the person is not in the map (e.g. blank
        // ""), upstream reduces to an empty actors-for-task; but the
        // loop still runs and crashes for blank "" (actors[""] is
        // undefined). HOWEVER in fixture 05 we see exactly one actor
        // circle drawn from `Sign Up: 5:` with empty name — because
        // the split on `:` gives `[""]` and `peeps.map(trim) = [""]`,
        // and upstream's `Set` still contains `""`, giving actor-0.
        // So blank actor names DO get rendered.
        let mut actor_circles = Vec::new();
        let mut x_pos = task_x + 14.0;
        for person in &t.people {
            // Find colour + pos in actor_pos table.
            let entry = actor_pos.iter().find(|(n, _, _)| n == person);
            if let Some((_, pos, colour)) = entry {
                actor_circles.push(JourneyActorChip {
                    pos: *pos,
                    cx: x_pos,
                    colour: colour.clone(),
                    title: person.clone(),
                });
                x_pos += 10.0;
            }
        }

        tasks.push(JourneyTaskLayout {
            index: i,
            x: task_x,
            y: task_y,
            fill: current_fill.clone(),
            num: current_num,
            task: t.task.clone(),
            score: t.score,
            face_cy,
            face,
            actor_circles,
        });
    }

    // ── Bounds ───────────────────────────────────────────────────
    // From `drawTasks`, each task inserts into `bounds.insert`:
    //   bounds.insert(task.x, task.y, task.x + task.width + conf.taskMargin, 300 + 5*30);
    // where task.width = conf.diagramMarginX (= 50). So stopx = task.x
    // + 50 + 50 = task.x + 100 = leftMargin + i*200 + 100.
    // stopy = 300 + 150 = 450. starty = taskPos = 110 (minimum).
    // bounds.data.starty stays at min inserted value (110).
    //
    // Final: height = box.stopy - box.starty + 2*diagramMarginY,
    //        width  = leftMargin + box.stopx + 2*diagramMarginX.
    // Activity line y = conf.height * 4 = 200; x2 = width - leftMargin - 4.
    let has_title = d.title.is_some() || d.meta.title.is_some();
    let (starty, stopy) = if d.tasks.is_empty() {
        // No tasks: upstream still produces a diagram with minimal
        // content. The 01 fixture confirms: starty=undefined, stopy=undefined,
        // startx=undefined, stopx=undefined → NaN arithmetic below.
        // Final: viewBox height = 450 - 110 = 340 + 2*10 = 360… but
        // 01.svg shows height 90 and viewBox height 90. So empty-task
        // flow: box.stopy=undefined, box.starty=undefined, stopy-starty=NaN
        // => NaN + 2*diagramMarginY = NaN. Hmm but the SVG says height=90.
        //
        // Inspect 01 more carefully: viewBox "0 -25 400 90" height=115.
        // So final height is 90. title_extra=70. 115 = 90 + 25. So the
        // logic is: for empty diagrams, upstream still inserts bounds
        // at (0,0,leftMargin,0) from `drawActorLegend` path:
        //   bounds.insert(0, 0, leftMargin, Object.keys(actors).length * 50);
        // When there are no actors: (0, 0, 250, 0).
        // But 01 has no actors, so stopy=0, starty=0 → height=0+20=20.
        // That doesn't match 90 either. Let me re-read…
        //
        // Actually 01 title="Adding journey diagram functionality…"
        // and section "Order from website" — but the mmd is just
        // "journey\ntitle …\nsection Order from website\n" with NO
        // tasks under it. So tasks=[]. drawTasks loops 0 times.
        // drawActorLegend: actors={}. bounds.insert(0,0,leftMargin,0)
        // with leftMargin=150+0=150+? Actually actorNames is empty, so
        // no actors are registered. Then leftMargin = 150 + 0 = 150.
        // bounds.insert(0, 0, 150, 0) → {startx:0, starty:0, stopx:150,
        // stopy:0}. Then width = 150 + 150 + 2*50 = 400. ✓
        // height = 0 - 0 + 2*10 = 20. But 01 has height=90 (viewBox) /
        // 115 (attr). Extra for title = 70. 20 + 70 = 90 ✓ .. Great!
        //
        // But the arrow is at y=200 from x=150 to x=246. Width=400
        // gives x2 = 400 - 150 - 4 = 246. ✓
        (0.0, 0.0)
    } else {
        // Actor legend insert: (0, 0, leftMargin, N*50) where N=actor count.
        // Tasks insert: (task.x, task.y, task.x + 50 + 50, 450).
        // bounds.insert uses Math.min/Math.max across all. So:
        //   starty = min(0, 110, ...) = 0
        //   stopy  = max(N*50, 450, ...) = 450 (since max N=6 => 300 < 450)
        //
        // If there are MANY actors (N*50 > 450), stopy bumps. Check 09:
        // 3 actors → 150. 450 > 150. stopy=450. ✓
        //
        // However wait — fixture 02 shows height=540 viewBox. Let's
        // verify: 2 actors (Cat,Me). actor insert (0,0,leftMargin,100).
        // stopy = max(100, 450) = 450. starty = min(0, 110) = 0.
        // height = 450 - 0 + 2*10 = 470. But 02 viewBox shows 540,
        // height attr 565 (540+25). 470 != 540. Hmm.
        //
        // Missing 70 extra for title? 470 + 70 = 540 ✓ .
        (0.0, 450.0)
    };

    let n_actors = actor_pos.len();
    // bounds.insert(0,0,leftMargin, n_actors*50)
    let actor_stopy = (n_actors as f64) * 50.0;
    let stopy = if d.tasks.is_empty() {
        // No tasks: only actor-legend insert ran.
        actor_stopy.max(stopy)
    } else {
        stopy.max(actor_stopy)
    };

    let stopx = if d.tasks.is_empty() {
        left_margin
    } else {
        // Upstream `drawTasks` sets `task.width = conf.diagramMarginX`
        // *for the bounds insert only* — the visible task rectangle is
        // still `conf.width` (150). bounds.insert(task.x, _, task.x +
        // task.width + taskMargin, _) gives stopx = lastTask.x + 50 +
        // 50 = lastTask.x + 100.
        let last = d.tasks.len() - 1;
        let last_x = last as f64 * conf.task_margin + last as f64 * conf.width + left_margin;
        (last_x + conf.diagram_margin_x + conf.task_margin).max(left_margin)
    };

    let height = stopy - starty + 2.0 * conf.diagram_margin_y;
    let width = left_margin + stopx + 2.0 * conf.diagram_margin_x;

    let arrow_y = conf.height * 4.0;
    let arrow_x1 = left_margin;
    let arrow_x2 = width - left_margin - 4.0;

    let title_extra = if has_title { 70.0 } else { 0.0 };

    Ok(JourneyLayout {
        width,
        height,
        title_extra,
        left_margin,
        has_title,
        title: d
            .title
            .clone()
            .or_else(|| d.meta.title.clone())
            .unwrap_or_default(),
        title_color: conf.title_color.clone(),
        title_font_family: conf.title_font_family.clone(),
        title_font_size: conf.title_font_size.clone(),
        actors: legend,
        sections,
        tasks,
        task_font_family: conf.task_font_family.clone(),
        task_font_size: conf.task_font_size,
        arrow_x1,
        arrow_x2,
        arrow_y,
    })
}

/// Measure width using the jsdom fallback (14px sans-serif).
pub fn jsdom_width(s: &str) -> f64 {
    text_width(s, JSDOM_FALLBACK_FAMILY, JSDOM_FALLBACK_SIZE, false, false)
}

/// Port of upstream `drawActorLegend` wrap algorithm.
///
/// ```text
/// if fullWidth <= max: lines = [text]
/// else:
///   words = text.split(' ')
///   for each word:
///     test = cur? cur + ' ' + word : word
///     if width(test) > max:
///       if cur: push cur
///       cur = word
///       if width(word) > max:
///         broken = ''
///         for each char:
///           broken += char
///           if width(broken + '-') > max:
///             push broken[:-1] + '-'
///             broken = char
///         cur = broken
///     else:
///       cur = test
///   if cur: push cur
/// ```
pub fn wrap_text(text: &str, max: f64) -> Vec<String> {
    if jsdom_width(text) <= max {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        let test = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", cur, word)
        };
        if jsdom_width(&test) > max {
            if !cur.is_empty() {
                lines.push(cur.clone());
            }
            cur = word.to_string();

            // If the word itself overflows, hyphen-break.
            if jsdom_width(word) > max {
                let mut broken = String::new();
                for ch in word.chars() {
                    broken.push(ch);
                    // Test if broken + '-' exceeds max.
                    let mut test = broken.clone();
                    test.push('-');
                    if jsdom_width(&test) > max {
                        let mut pushed: String =
                            broken.chars().take(broken.chars().count() - 1).collect();
                        pushed.push('-');
                        lines.push(pushed);
                        broken = ch.to_string();
                    }
                }
                cur = broken;
            }
        } else {
            cur = test;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_short_is_single_line() {
        let lines = wrap_text("hi", 1000.0);
        assert_eq!(lines, vec!["hi".to_string()]);
    }

    #[test]
    fn wrap_hyphenates_long_word() {
        // Long word with small max should hyphen-break.
        let lines = wrap_text("Supercalifragilistic", 40.0);
        assert!(lines.len() >= 2);
        // At least one line should end with '-'.
        assert!(lines.iter().take(lines.len() - 1).all(|l| l.ends_with('-')));
    }
}
