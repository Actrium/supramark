use crate::model::ditaa::{DitaaDiagram, DitaaOptions};
use crate::Result;

#[derive(Debug, Clone)]
pub struct DitaaLayout {
    pub boxes: Vec<DitaaBox>,
    pub lines: Vec<DitaaLine>,
    pub texts: Vec<DitaaText>,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct DitaaBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub round: bool,
    pub color: Option<String>,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DitaaLine {
    pub points: Vec<(f64, f64)>,
    pub dashed: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
}

#[derive(Debug, Clone)]
pub struct DitaaText {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

const CHAR_W: f64 = 8.0;
const CHAR_H: f64 = 14.0;

pub fn layout_ditaa(diagram: &DitaaDiagram) -> Result<DitaaLayout> {
    let scale = diagram.options.scale.unwrap_or(1.0);
    let char_w = CHAR_W * scale;
    let char_h = CHAR_H * scale;
    let grid = build_grid(&diagram.source);
    let rows = grid.len();
    let cols = grid.iter().map(std::vec::Vec::len).max().unwrap_or(0);
    let mut used = vec![vec![false; cols]; rows];

    let boxes = detect_boxes(&grid, &mut used, &diagram.options, char_w, char_h);
    let lines = detect_lines(&grid, &mut used, char_w, char_h);
    let texts = detect_texts(&grid, &used, char_w, char_h);

    Ok(DitaaLayout {
        boxes,
        lines,
        texts,
        width: cols as f64 * char_w + char_w,
        height: rows as f64 * char_h + char_h,
    })
}

fn build_grid(source: &str) -> Vec<Vec<char>> {
    source.lines().map(|line| line.chars().collect()).collect()
}

fn detect_boxes(
    grid: &[Vec<char>],
    used: &mut [Vec<bool>],
    options: &DitaaOptions,
    char_w: f64,
    char_h: f64,
) -> Vec<DitaaBox> {
    let mut boxes = Vec::new();
    for row in 0..grid.len() {
        for col in 0..grid[row].len() {
            if grid[row][col] != '+' || used[row][col] {
                continue;
            }
            if let Some((row2, col2)) = try_detect_box(grid, used, row, col) {
                mark_used(used, row, col, row2, col2);
                boxes.push(DitaaBox {
                    x: col as f64 * char_w,
                    y: row as f64 * char_h,
                    width: (col2 - col) as f64 * char_w,
                    height: (row2 - row) as f64 * char_h,
                    round: options.round_corners,
                    color: find_box_color(grid, row, col, row2, col2),
                    text: extract_box_text(grid, row, col, row2, col2),
                });
            }
        }
    }
    boxes
}

fn try_detect_box(
    grid: &[Vec<char>],
    used: &[Vec<bool>],
    row: usize,
    col: usize,
) -> Option<(usize, usize)> {
    let mut col2 = col + 1;
    while col2 < grid[row].len() && matches!(grid[row][col2], '-' | '=') {
        col2 += 1;
    }
    if col2 >= grid[row].len() || grid[row][col2] != '+' || col2 <= col + 1 {
        return None;
    }

    let mut row2 = row + 1;
    while row2 < grid.len() {
        let ch = grid[row2].get(col).copied().unwrap_or(' ');
        if ch == '+' {
            break;
        }
        if !matches!(ch, '|' | ':') {
            return None;
        }
        row2 += 1;
    }
    if row2 >= grid.len() || grid[row2].get(col).copied() != Some('+') || row2 <= row + 1 {
        return None;
    }

    for grid_row in grid.iter().take(row2).skip(row + 1) {
        if !matches!(grid_row.get(col2).copied().unwrap_or(' '), '|' | ':') {
            return None;
        }
    }
    for check_col in col + 1..col2 {
        if !matches!(grid[row2].get(check_col).copied().unwrap_or(' '), '-' | '=') {
            return None;
        }
    }
    for used_row in used.iter().take(row2 + 1).skip(row) {
        for check_col in col..=col2 {
            if used_row.get(check_col).copied().unwrap_or(false) {
                return None;
            }
        }
    }
    Some((row2, col2))
}

fn mark_used(used: &mut [Vec<bool>], row1: usize, col1: usize, row2: usize, col2: usize) {
    for used_row in used.iter_mut().take(row2 + 1).skip(row1) {
        for col in col1..=col2 {
            if col < used_row.len() {
                used_row[col] = true;
            }
        }
    }
}

fn find_box_color(
    grid: &[Vec<char>],
    row1: usize,
    col1: usize,
    row2: usize,
    col2: usize,
) -> Option<String> {
    for grid_row in grid.iter().take(row2).skip(row1 + 1) {
        for col in col1 + 1..col2 {
            if grid_row.get(col).copied() == Some('c') {
                let code: String = (1..=3)
                    .filter_map(|offset| grid_row.get(col + offset).copied())
                    .collect();
                if code.len() == 3 {
                    return Some(color_code_to_hex(&code));
                }
            }
        }
    }
    None
}

fn extract_box_text(
    grid: &[Vec<char>],
    row1: usize,
    col1: usize,
    row2: usize,
    col2: usize,
) -> Option<String> {
    let mut lines = Vec::new();
    for grid_row in grid.iter().take(row2).skip(row1 + 1) {
        let line: String = (col1 + 1..col2)
            .filter_map(|col| grid_row.get(col).copied())
            .collect();
        let cleaned = remove_color_codes(line.trim());
        if !cleaned.is_empty() {
            lines.push(cleaned.to_string());
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn remove_color_codes(input: &str) -> &str {
    let trimmed = input.trim();
    // `len()` counts bytes, so byte 4 can land inside a character; the code
    // being looked for is three ASCII alphanumerics.
    if trimmed.len() >= 4 && trimmed.starts_with('c') && trimmed.is_char_boundary(4) {
        let code = &trimmed[1..4];
        if code.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return trimmed[4..].trim_start();
        }
    }
    trimmed
}

fn color_code_to_hex(code: &str) -> String {
    match code {
        "BLU" => "#6666FF".to_string(),
        "RED" => "#FF6666".to_string(),
        "GRE" => "#66CC66".to_string(),
        "YEL" => "#FFFF66".to_string(),
        _ => "#EEEEEE".to_string(),
    }
}

fn detect_lines(
    grid: &[Vec<char>],
    used: &mut [Vec<bool>],
    char_w: f64,
    char_h: f64,
) -> Vec<DitaaLine> {
    let mut lines = Vec::new();

    for row in 0..grid.len() {
        let mut col = 0;
        while col < grid[row].len() {
            if used[row].get(col).copied().unwrap_or(false) {
                col += 1;
                continue;
            }
            if let Some((line, next_col)) =
                detect_horizontal_line(grid, used, row, col, char_w, char_h)
            {
                lines.push(line);
                col = next_col;
                continue;
            }
            col += 1;
        }
    }

    let cols = grid.iter().map(std::vec::Vec::len).max().unwrap_or(0);
    for col in 0..cols {
        let mut row = 0;
        while row < grid.len() {
            if used[row].get(col).copied().unwrap_or(false) {
                row += 1;
                continue;
            }
            if let Some((line, next_row)) =
                detect_vertical_line(grid, used, row, col, char_w, char_h)
            {
                lines.push(line);
                row = next_row;
                continue;
            }
            row += 1;
        }
    }

    lines
}

fn detect_horizontal_line(
    grid: &[Vec<char>],
    used: &mut [Vec<bool>],
    row: usize,
    start_col: usize,
    char_w: f64,
    char_h: f64,
) -> Option<(DitaaLine, usize)> {
    let first = *grid[row].get(start_col)?;
    if !matches!(first, '<' | '>' | '-' | '=') {
        return None;
    }

    let mut col = start_col;
    let mut arrow_start = false;
    let mut dashed = false;
    if first == '<' {
        arrow_start = true;
        col += 1;
    }

    let line_start = col;
    while col < grid[row].len() && matches!(grid[row][col], '-' | '=') {
        dashed |= grid[row][col] == '=';
        col += 1;
    }
    if col == line_start {
        return None;
    }

    let mut arrow_end = false;
    if grid[row].get(col).copied() == Some('>') {
        arrow_end = true;
        col += 1;
    }

    for slot in &mut used[row][start_col..col] {
        *slot = true;
    }

    let y = row as f64 * char_h + char_h / 2.0;
    let x1 = start_col as f64 * char_w;
    let x2 = col as f64 * char_w;
    Some((
        DitaaLine {
            points: vec![(x1, y), (x2, y)],
            dashed,
            arrow_start,
            arrow_end,
        },
        col,
    ))
}

fn detect_vertical_line(
    grid: &[Vec<char>],
    used: &mut [Vec<bool>],
    start_row: usize,
    col: usize,
    char_w: f64,
    char_h: f64,
) -> Option<(DitaaLine, usize)> {
    let first = grid.get(start_row)?.get(col).copied().unwrap_or(' ');
    if !matches!(first, '^' | 'v' | '|' | ':') {
        return None;
    }

    let mut row = start_row;
    let mut arrow_start = false;
    let mut dashed = false;
    if first == '^' {
        arrow_start = true;
        row += 1;
    }

    let line_start = row;
    while row < grid.len() && matches!(grid[row].get(col).copied().unwrap_or(' '), '|' | ':') {
        dashed |= grid[row].get(col).copied().unwrap_or(' ') == ':';
        row += 1;
    }
    if row == line_start {
        return None;
    }

    let mut arrow_end = false;
    if grid.get(row).and_then(|line| line.get(col)).copied() == Some('v') {
        arrow_end = true;
        row += 1;
    }

    for used_row in used.iter_mut().take(row.min(grid.len())).skip(start_row) {
        if col < used_row.len() {
            used_row[col] = true;
        }
    }

    let x = col as f64 * char_w + char_w / 2.0;
    let y1 = start_row as f64 * char_h;
    let y2 = row as f64 * char_h;
    Some((
        DitaaLine {
            points: vec![(x, y1), (x, y2)],
            dashed,
            arrow_start,
            arrow_end,
        },
        row,
    ))
}

fn detect_texts(
    grid: &[Vec<char>],
    used: &[Vec<bool>],
    char_w: f64,
    char_h: f64,
) -> Vec<DitaaText> {
    let mut texts = Vec::new();
    for (row_idx, row) in grid.iter().enumerate() {
        let mut col = 0;
        while col < row.len() {
            if used[row_idx].get(col).copied().unwrap_or(false)
                || row[col].is_whitespace()
                || is_shape_char(row[col])
            {
                col += 1;
                continue;
            }
            let start = col;
            let mut text = String::new();
            while col < row.len()
                && !used[row_idx].get(col).copied().unwrap_or(false)
                && !row[col].is_whitespace()
                && !is_shape_char(row[col])
            {
                text.push(row[col]);
                col += 1;
            }
            if !text.is_empty() {
                texts.push(DitaaText {
                    x: start as f64 * char_w,
                    y: row_idx as f64 * char_h + char_h * 0.8,
                    text,
                });
            }
        }
    }
    texts
}

fn is_shape_char(ch: char) -> bool {
    matches!(ch, '+' | '-' | '=' | '|' | ':' | '<' | '>' | '^' | 'v')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ditaa::{DitaaDiagram, DitaaOptions};

    #[test]
    fn layout_detects_box_and_arrow() {
        let diagram = DitaaDiagram {
            source: "+--+  +--+\n|A |->|B |\n+--+  +--+".to_string(),
            options: DitaaOptions::default(),
        };
        let layout = layout_ditaa(&diagram).unwrap();
        assert_eq!(layout.boxes.len(), 2);
        assert!(!layout.lines.is_empty());
    }

    #[test]
    fn layout_extracts_text_outside_shapes() {
        let diagram = DitaaDiagram {
            source: "hello\n  |\n  v".to_string(),
            options: DitaaOptions::default(),
        };
        let layout = layout_ditaa(&diagram).unwrap();
        assert!(layout.texts.iter().any(|text| text.text == "hello"));
    }
}
