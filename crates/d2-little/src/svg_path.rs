use crate::geo::{BezierCurve, PathElement, Point, Segment};

/// Mirror Go `lib/svg/path.go`'s `chopPrecision`: bring the value down to
/// float32 precision, divide by 10000, and *then* round to nearest integer
/// via `math.Round`. The net effect is that every shape path coordinate
/// collapses to an integer — page-shape control points like `0.456297`
/// end up as `0`, and `74.1836` ends up as `74`. This is what keeps Go's
/// generated SVG paths byte-identical to the expected fixtures (which are
/// all integer-valued).
fn chop_precision(f: f64) -> f64 {
    let scaled = (f * 10000.0) as f32;
    let result = (scaled as f64 / 10000.0).round();
    if result == 0.0 { 0.0 } else { result }
}

/// SVG path building state machine.
///
/// Tracks the current drawing position, path elements, and accumulated
/// SVG path commands. Supports coordinate scaling via `scale_x` / `scale_y`.
pub struct SvgPathContext {
    pub path: Vec<PathElement>,
    pub commands: Vec<String>,
    pub start: Option<Point>,
    pub current: Option<Point>,
    pub top_left: Point,
    pub scale_x: f64,
    pub scale_y: f64,
}

impl SvgPathContext {
    pub fn new(top_left: Point, scale_x: f64, scale_y: f64) -> Self {
        Self {
            path: Vec::new(),
            commands: Vec::new(),
            start: None,
            current: None,
            top_left,
            scale_x,
            scale_y,
        }
    }

    /// Compute a point relative to `base` by applying scale factors to `dx`/`dy`.
    pub fn relative(&self, base: &Point, dx: f64, dy: f64) -> Point {
        Point::new(
            chop_precision(base.x + self.scale_x * dx),
            chop_precision(base.y + self.scale_y * dy),
        )
    }

    /// Compute an absolute point relative to `top_left`.
    pub fn absolute(&self, x: f64, y: f64) -> Point {
        let tl = self.top_left;
        self.relative(&tl, x, y)
    }

    /// Begin a new sub-path at point `p` (SVG "M" command).
    pub fn start_at(&mut self, p: Point) {
        self.start = Some(p);
        self.commands.push(format!("M {} {}", p.x, p.y));
        self.current = Some(p);
    }

    /// Close the current sub-path (SVG "Z" command).
    pub fn z(&mut self) {
        let current = self.current.expect("Z called without current point");
        let start = self.start.expect("Z called without start point");
        self.path.push(PathElement::Segment(Segment {
            start: current,
            end: start,
        }));
        self.commands.push("Z".to_string());
        self.current = Some(start);
    }

    /// Line-to command (SVG "L").
    ///
    /// If `is_lower_case` is true, `x` and `y` are relative to the current point.
    /// Otherwise they are absolute (relative to `top_left`).
    pub fn l(&mut self, is_lower_case: bool, x: f64, y: f64) {
        let current = self.current.expect("L called without current point");
        let end_point = if is_lower_case {
            self.relative(&current, x, y)
        } else {
            self.absolute(x, y)
        };
        self.path.push(PathElement::Segment(Segment {
            start: current,
            end: end_point,
        }));
        self.commands
            .push(format!("L {} {}", end_point.x, end_point.y));
        self.current = Some(end_point);
    }

    /// Cubic Bezier curve command (SVG "C").
    ///
    /// `ctrls` are three control-point pairs `[(x1,y1), (x2,y2), (x3,y3)]`.
    /// If `is_lower_case`, offsets are relative to the current point.
    pub fn c(&mut self, is_lower_case: bool, ctrls: [(f64, f64); 3]) {
        let current = self.current.expect("C called without current point");
        let p = |x: f64, y: f64| -> Point {
            if is_lower_case {
                self.relative(&current, x, y)
            } else {
                self.absolute(x, y)
            }
        };
        let p1 = current;
        let p2 = p(ctrls[0].0, ctrls[0].1);
        let p3 = p(ctrls[1].0, ctrls[1].1);
        let p4 = p(ctrls[2].0, ctrls[2].1);
        let points = vec![p1, p2, p3, p4];
        self.path
            .push(PathElement::Bezier(BezierCurve::new(points.clone())));
        self.commands.push(format!(
            "C {} {} {} {} {} {}",
            points[1].x, points[1].y, points[2].x, points[2].y, points[3].x, points[3].y,
        ));
        self.current = Some(points[3]);
    }

    /// Horizontal line-to command (SVG "H").
    pub fn h(&mut self, is_lower_case: bool, x: f64) {
        let current = self.current.expect("H called without current point");
        let end_point = if is_lower_case {
            self.relative(&current, x, 0.0)
        } else {
            let mut pt = self.absolute(x, 0.0);
            pt.y = current.y;
            pt
        };
        self.path.push(PathElement::Segment(Segment {
            start: current,
            end: end_point,
        }));
        self.commands.push(format!("H {}", end_point.x));
        self.current = Some(end_point);
    }

    /// Vertical line-to command (SVG "V").
    pub fn v(&mut self, is_lower_case: bool, y: f64) {
        let current = self.current.expect("V called without current point");
        let end_point = if is_lower_case {
            self.relative(&current, 0.0, y)
        } else {
            let mut pt = self.absolute(0.0, y);
            pt.x = current.x;
            pt
        };
        self.path.push(PathElement::Segment(Segment {
            start: current,
            end: end_point,
        }));
        self.commands.push(format!("V {}", end_point.y));
        self.current = Some(end_point);
    }

    /// Generate the complete SVG path data string.
    pub fn path_data(&self) -> String {
        self.commands.join(" ")
    }
}

/// Calculate stroke-dash attributes for a given stroke width and dash-gap size.
///
/// As the stroke width gets thicker, the dash gap gets smaller.
/// Returns `(dash_size, gap_size)`.
pub fn get_stroke_dash_attributes(stroke_width: f64, dash_gap_size: f64) -> (f64, f64) {
    let scale = (-0.6 * stroke_width + 10.6).log10() * 0.5 + 0.5;
    let scaled_dash_size = stroke_width * dash_gap_size;
    let scaled_gap_size = scale * scaled_dash_size;
    (scaled_dash_size, scaled_gap_size)
}

/// De Casteljau's algorithm: extract the sub-segment of a cubic Bezier
/// curve defined by control points `(p1, p2, p3, p4)` from parameter
/// `t0` to `t1` (where 0 <= t0 < t1 <= 1).
///
/// Returns the four new control points `(q1, q2, q3, q4)`.
pub fn bezier_curve_segment(
    p1: &Point,
    p2: &Point,
    p3: &Point,
    p4: &Point,
    t0: f64,
    t1: f64,
) -> (Point, Point, Point, Point) {
    let u0 = 1.0 - t0;
    let u1 = 1.0 - t1;

    let q1 = Point {
        x: (u0 * u0 * u0) * p1.x
            + (3.0 * t0 * u0 * u0) * p2.x
            + (3.0 * t0 * t0 * u0) * p3.x
            + t0 * t0 * t0 * p4.x,
        y: (u0 * u0 * u0) * p1.y
            + (3.0 * t0 * u0 * u0) * p2.y
            + (3.0 * t0 * t0 * u0) * p3.y
            + t0 * t0 * t0 * p4.y,
    };
    let q2 = Point {
        x: (u0 * u0 * u1) * p1.x
            + (2.0 * t0 * u0 * u1 + u0 * u0 * t1) * p2.x
            + (t0 * t0 * u1 + 2.0 * u0 * t0 * t1) * p3.x
            + t0 * t0 * t1 * p4.x,
        y: (u0 * u0 * u1) * p1.y
            + (2.0 * t0 * u0 * u1 + u0 * u0 * t1) * p2.y
            + (t0 * t0 * u1 + 2.0 * u0 * t0 * t1) * p3.y
            + t0 * t0 * t1 * p4.y,
    };
    let q3 = Point {
        x: (u0 * u1 * u1) * p1.x
            + (t0 * u1 * u1 + 2.0 * u0 * t1 * u1) * p2.x
            + (2.0 * t0 * t1 * u1 + u0 * t1 * t1) * p3.x
            + t0 * t1 * t1 * p4.x,
        y: (u0 * u1 * u1) * p1.y
            + (t0 * u1 * u1 + 2.0 * u0 * t1 * u1) * p2.y
            + (2.0 * t0 * t1 * u1 + u0 * t1 * t1) * p3.y
            + t0 * t1 * t1 * p4.y,
    };
    let q4 = Point {
        x: (u1 * u1 * u1) * p1.x
            + (3.0 * t1 * u1 * u1) * p2.x
            + (3.0 * t1 * t1 * u1) * p3.x
            + t1 * t1 * t1 * p4.x,
        y: (u1 * u1 * u1) * p1.y
            + (3.0 * t1 * u1 * u1) * p2.y
            + (3.0 * t1 * t1 * u1) * p3.y
            + t1 * t1 * t1 * p4.y,
    };

    (q1, q2, q3, q4)
}

/// Escape text for safe inclusion in SVG/XML.
///
/// Replaces `<`, `>`, `&`, `'`, and `"` with their XML entity equivalents.
pub fn escape_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\'' => result.push_str("&#39;"),
            '"' => result.push_str("&#34;"),
            '\t' => result.push_str("&#x9;"),
            '\n' => result.push_str("&#xA;"),
            '\r' => result.push_str("&#xD;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Encode `text` as a base32 (RFC 4648) string with padding stripped,
/// suitable for use as an SVG element ID.
pub fn svg_id(text: &str) -> String {
    use base32::Alphabet;
    base32::encode(Alphabet::Rfc4648 { padding: false }, text.as_bytes())
}

/// SVG path command type with its parameter count for parsing.
pub enum SvgPathCommand {
    M,
    L,
    C,
    S,
}

impl SvgPathCommand {
    /// Number of string tokens this command consumes (including the command letter).
    pub fn increment(&self) -> usize {
        match self {
            Self::M => 3,
            Self::L => 3,
            Self::C => 7,
            Self::S => 5,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "M" => Some(Self::M),
            "L" => Some(Self::L),
            "C" => Some(Self::C),
            "S" => Some(Self::S),
            _ => None,
        }
    }

    /// Build the SVG path substring for this command starting at `offset` in `data`.
    pub fn path_string(&self, offset: usize, data: &[&str]) -> String {
        match self {
            Self::M => format!("M {} {} ", data[offset + 1], data[offset + 2]),
            Self::L => format!("L {} {} ", data[offset + 1], data[offset + 2]),
            Self::C => format!(
                "C {} {} {} {} {} {} ",
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
            ),
            Self::S => format!(
                "S {} {} {} {} ",
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
            ),
        }
    }
}

/// Compute the total approximate length of an SVG path string.
pub fn path_length(path_data: &[&str]) -> Result<f64, String> {
    #[allow(unused_assignments)]
    let mut x: f64 = 0.0;
    #[allow(unused_assignments)]
    let mut y: f64 = 0.0;
    let mut total: f64 = 0.0;
    let mut prev = Point::new(0.0, 0.0);
    let mut i = 0;

    while i < path_data.len() {
        let cmd = SvgPathCommand::parse(path_data[i])
            .ok_or_else(|| format!("unknown svg path command \"{}\"", path_data[i]))?;
        match cmd {
            SvgPathCommand::M => {
                x = path_data[i + 1].parse().unwrap_or(0.0);
                y = path_data[i + 2].parse().unwrap_or(0.0);
            }
            SvgPathCommand::L => {
                x = path_data[i + 1].parse().unwrap_or(0.0);
                y = path_data[i + 2].parse().unwrap_or(0.0);
                total += crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
            SvgPathCommand::C => {
                x = path_data[i + 5].parse().unwrap_or(0.0);
                y = path_data[i + 6].parse().unwrap_or(0.0);
                total += crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
            SvgPathCommand::S => {
                x = path_data[i + 3].parse().unwrap_or(0.0);
                y = path_data[i + 4].parse().unwrap_or(0.0);
                total += crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
        }
        prev = Point::new(x, y);
        i += cmd.increment();
    }

    Ok(total)
}

/// Split an SVG path into two sub-paths, where the first sub-path is
/// approximately `percentage` (0.0..1.0) of the total path length.
pub fn split_path(path: &str, percentage: f64) -> Result<(String, String), String> {
    let tokens: Vec<&str> = path.split_whitespace().collect();
    let total_len = path_length(&tokens)?;

    let mut sum_lens: f64 = 0.0;
    let mut cur_len: f64;
    #[allow(unused_assignments)]
    let mut x: f64 = 0.0;
    #[allow(unused_assignments)]
    let mut y: f64 = 0.0;
    let mut prev = Point::new(0.0, 0.0);
    let mut path1 = String::new();
    let mut path2 = String::new();
    let mut past_half = false;
    let mut i = 0;

    while i < tokens.len() {
        let cmd = SvgPathCommand::parse(tokens[i])
            .ok_or_else(|| format!("unknown svg path command \"{}\"", tokens[i]))?;

        match cmd {
            SvgPathCommand::M => {
                x = tokens[i + 1].parse().unwrap_or(0.0);
                y = tokens[i + 2].parse().unwrap_or(0.0);
                cur_len = 0.0;
            }
            SvgPathCommand::L => {
                x = tokens[i + 1].parse().unwrap_or(0.0);
                y = tokens[i + 2].parse().unwrap_or(0.0);
                cur_len = crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
            SvgPathCommand::C => {
                x = tokens[i + 5].parse().unwrap_or(0.0);
                y = tokens[i + 6].parse().unwrap_or(0.0);
                cur_len = crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
            SvgPathCommand::S => {
                x = tokens[i + 3].parse().unwrap_or(0.0);
                y = tokens[i + 4].parse().unwrap_or(0.0);
                cur_len = crate::geo::euclidean_distance(prev.x, prev.y, x, y);
            }
        }

        let cur_path = cmd.path_string(i, &tokens);
        sum_lens += cur_len;

        if past_half {
            path2.push_str(&cur_path);
        } else if sum_lens < total_len * percentage {
            path1.push_str(&cur_path);
        } else {
            let t = (total_len * percentage - sum_lens + cur_len) / cur_len;

            match cmd {
                SvgPathCommand::M => {
                    if i + 4 < tokens.len() {
                        path2.push_str(&format!("M {} {} ", tokens[i + 3], tokens[i + 4]));
                    }
                }
                SvgPathCommand::L => {
                    let nx = (x - prev.x) * t + prev.x;
                    let ny = (y - prev.y) * t + prev.y;
                    path1.push_str(&format!("L {:.6} {:.6} ", nx, ny));
                    path2.push_str(&format!("M {:.6} {:.6} L {:.6} {:.6} ", nx, ny, x, y));
                }
                SvgPathCommand::C => {
                    let h1x: f64 = tokens[i + 1].parse().unwrap_or(0.0);
                    let h1y: f64 = tokens[i + 2].parse().unwrap_or(0.0);
                    let h2x: f64 = tokens[i + 3].parse().unwrap_or(0.0);
                    let h2y: f64 = tokens[i + 4].parse().unwrap_or(0.0);
                    let heading1 = Point::new(h1x, h1y);
                    let heading2 = Point::new(h2x, h2y);
                    let next_pt = Point::new(x, y);

                    let (_, q2, q3, q4) =
                        bezier_curve_segment(&prev, &heading1, &heading2, &next_pt, 0.0, 0.5);
                    path1.push_str(&format!(
                        "C {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} ",
                        q2.x, q2.y, q3.x, q3.y, q4.x, q4.y
                    ));

                    let (q1, q2, q3, q4) =
                        bezier_curve_segment(&prev, &heading1, &heading2, &next_pt, 0.5, 1.0);
                    path2.push_str(&format!(
                        "M {:.6} {:.6} C {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} ",
                        q1.x, q1.y, q2.x, q2.y, q3.x, q3.y, q4.x, q4.y
                    ));
                }
                SvgPathCommand::S => {
                    path1.push_str(&format!(
                        "S {} {} {} {} ",
                        tokens[i + 1],
                        tokens[i + 2],
                        tokens[i + 3],
                        tokens[i + 4],
                    ));
                    path2.push_str(&format!("M {} {} ", tokens[i + 3], tokens[i + 4]));
                }
            }
            past_half = true;
        }

        i += cmd.increment();
        prev = Point::new(x, y);
    }

    Ok((path1, path2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chop_precision() {
        // Go `math.Round` rounds to the nearest integer, so
        // chopPrecision always lands on a whole number.
        assert_eq!(chop_precision(1.23456), 1.0);
        assert_eq!(chop_precision(74.1836), 74.0);
        assert_eq!(chop_precision(16.5), 17.0);
        assert_eq!(chop_precision(-0.0), 0.0);
        assert_eq!(chop_precision(0.0), 0.0);
    }

    #[test]
    fn test_svg_path_context_basic() {
        let mut ctx = SvgPathContext::new(Point::new(0.0, 0.0), 1.0, 1.0);
        ctx.start_at(Point::new(10.0, 20.0));
        ctx.l(false, 30.0, 40.0);
        ctx.z();
        assert_eq!(ctx.path_data(), "M 10 20 L 30 40 Z");
    }

    #[test]
    fn test_svg_path_context_hv() {
        let mut ctx = SvgPathContext::new(Point::new(0.0, 0.0), 1.0, 1.0);
        ctx.start_at(Point::new(10.0, 20.0));
        ctx.h(false, 50.0);
        ctx.v(false, 80.0);
        let data = ctx.path_data();
        assert!(data.contains("M 10 20"));
        assert!(data.contains("H 50"));
        assert!(data.contains("V 80"));
    }

    #[test]
    fn test_svg_path_context_cubic() {
        let mut ctx = SvgPathContext::new(Point::new(0.0, 0.0), 1.0, 1.0);
        ctx.start_at(Point::new(0.0, 0.0));
        ctx.c(false, [(10.0, 20.0), (30.0, 40.0), (50.0, 60.0)]);
        let data = ctx.path_data();
        assert!(data.starts_with("M 0 0 C 10 20 30 40 50 60"));
    }

    #[test]
    fn test_svg_path_context_relative() {
        let mut ctx = SvgPathContext::new(Point::new(100.0, 200.0), 1.0, 1.0);
        ctx.start_at(Point::new(100.0, 200.0));
        ctx.l(true, 10.0, 20.0);
        let data = ctx.path_data();
        assert_eq!(data, "M 100 200 L 110 220");
    }

    #[test]
    fn test_svg_path_context_scale() {
        let ctx = SvgPathContext::new(Point::new(0.0, 0.0), 2.0, 3.0);
        let p = ctx.absolute(10.0, 10.0);
        assert_eq!(p.x, 20.0);
        assert_eq!(p.y, 30.0);
    }

    #[test]
    fn test_bezier_curve_segment_identity() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(1.0, 2.0);
        let p3 = Point::new(3.0, 2.0);
        let p4 = Point::new(4.0, 0.0);
        let (q1, _, _, q4) = bezier_curve_segment(&p1, &p2, &p3, &p4, 0.0, 1.0);
        assert!((q1.x - p1.x).abs() < 1e-10);
        assert!((q1.y - p1.y).abs() < 1e-10);
        assert!((q4.x - p4.x).abs() < 1e-10);
        assert!((q4.y - p4.y).abs() < 1e-10);
    }

    #[test]
    fn test_bezier_curve_segment_midpoint() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(0.0, 10.0);
        let p3 = Point::new(10.0, 10.0);
        let p4 = Point::new(10.0, 0.0);
        let (q1, _, _, q4) = bezier_curve_segment(&p1, &p2, &p3, &p4, 0.0, 0.5);
        assert!((q1.x - 0.0).abs() < 1e-10);
        assert!((q1.y - 0.0).abs() < 1e-10);
        // q4 should be the midpoint of the bezier at t=0.5
        assert!((q4.x - 5.0).abs() < 1e-10);
        assert!((q4.y - 7.5).abs() < 1e-10);
    }

    #[test]
    fn test_get_stroke_dash_attributes() {
        let (dash, gap) = get_stroke_dash_attributes(2.0, 5.0);
        assert!((dash - 10.0).abs() < 1e-10);
        assert!(gap > 0.0);
        assert!(gap < dash);
    }

    #[test]
    fn test_escape_text() {
        assert_eq!(escape_text("hello"), "hello");
        assert_eq!(escape_text("<b>hi</b>"), "&lt;b&gt;hi&lt;/b&gt;");
        assert_eq!(escape_text("a & b"), "a &amp; b");
        assert_eq!(escape_text("he said \"hi\""), "he said &#34;hi&#34;");
        assert_eq!(escape_text("it's"), "it&#39;s");
    }

    #[test]
    fn test_svg_id() {
        let id = svg_id("hello");
        assert!(!id.is_empty());
        assert!(!id.contains('='));
        // base32 of "hello" without padding
        assert_eq!(id, "NBSWY3DP");
    }

    #[test]
    fn test_path_length_simple() {
        let tokens = vec!["M", "0", "0", "L", "3", "4"];
        let len = path_length(&tokens).unwrap();
        assert!((len - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_split_path_basic() {
        let path = "M 0 0 L 10 0";
        let (p1, p2) = split_path(path, 0.5).unwrap();
        assert!(p1.contains("L"));
        assert!(p2.contains("M"));
    }
}
