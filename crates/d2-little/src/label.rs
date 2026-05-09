use crate::geo::Point;

/// Percentage locations where labels are placed along a connection.
pub const LEFT_LABEL_POSITION: f64 = 1.0 / 4.0;
pub const CENTER_LABEL_POSITION: f64 = 2.0 / 4.0;
pub const RIGHT_LABEL_POSITION: f64 = 3.0 / 4.0;

/// Space between a node border and its outside label.
pub const PADDING: f64 = 5.0;

/// Label position on shapes and edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i8)]
pub enum Position {
    #[default]
    Unset = 0,

    OutsideTopLeft,
    OutsideTopCenter,
    OutsideTopRight,

    OutsideLeftTop,
    OutsideLeftMiddle,
    OutsideLeftBottom,

    OutsideRightTop,
    OutsideRightMiddle,
    OutsideRightBottom,

    OutsideBottomLeft,
    OutsideBottomCenter,
    OutsideBottomRight,

    InsideTopLeft,
    InsideTopCenter,
    InsideTopRight,

    InsideMiddleLeft,
    InsideMiddleCenter,
    InsideMiddleRight,

    InsideBottomLeft,
    InsideBottomCenter,
    InsideBottomRight,

    BorderTopLeft,
    BorderTopCenter,
    BorderTopRight,

    BorderLeftTop,
    BorderLeftMiddle,
    BorderLeftBottom,

    BorderRightTop,
    BorderRightMiddle,
    BorderRightBottom,

    BorderBottomLeft,
    BorderBottomCenter,
    BorderBottomRight,

    UnlockedTop,
    UnlockedMiddle,
    UnlockedBottom,
}

impl Position {
    /// Parse a position string like `"OUTSIDE_TOP_LEFT"` into a [`Position`].
    ///
    /// Returns [`Position::Unset`] for unrecognized strings.
    pub fn from_string(s: &str) -> Self {
        match s {
            "OUTSIDE_TOP_LEFT" => Self::OutsideTopLeft,
            "OUTSIDE_TOP_CENTER" => Self::OutsideTopCenter,
            "OUTSIDE_TOP_RIGHT" => Self::OutsideTopRight,

            "OUTSIDE_LEFT_TOP" => Self::OutsideLeftTop,
            "OUTSIDE_LEFT_MIDDLE" => Self::OutsideLeftMiddle,
            "OUTSIDE_LEFT_BOTTOM" => Self::OutsideLeftBottom,

            "OUTSIDE_RIGHT_TOP" => Self::OutsideRightTop,
            "OUTSIDE_RIGHT_MIDDLE" => Self::OutsideRightMiddle,
            "OUTSIDE_RIGHT_BOTTOM" => Self::OutsideRightBottom,

            "OUTSIDE_BOTTOM_LEFT" => Self::OutsideBottomLeft,
            "OUTSIDE_BOTTOM_CENTER" => Self::OutsideBottomCenter,
            "OUTSIDE_BOTTOM_RIGHT" => Self::OutsideBottomRight,

            "INSIDE_TOP_LEFT" => Self::InsideTopLeft,
            "INSIDE_TOP_CENTER" => Self::InsideTopCenter,
            "INSIDE_TOP_RIGHT" => Self::InsideTopRight,

            "INSIDE_MIDDLE_LEFT" => Self::InsideMiddleLeft,
            "INSIDE_MIDDLE_CENTER" => Self::InsideMiddleCenter,
            "INSIDE_MIDDLE_RIGHT" => Self::InsideMiddleRight,

            "INSIDE_BOTTOM_LEFT" => Self::InsideBottomLeft,
            "INSIDE_BOTTOM_CENTER" => Self::InsideBottomCenter,
            "INSIDE_BOTTOM_RIGHT" => Self::InsideBottomRight,

            "BORDER_TOP_LEFT" => Self::BorderTopLeft,
            "BORDER_TOP_CENTER" => Self::BorderTopCenter,
            "BORDER_TOP_RIGHT" => Self::BorderTopRight,

            "BORDER_LEFT_TOP" => Self::BorderLeftTop,
            "BORDER_LEFT_MIDDLE" => Self::BorderLeftMiddle,
            "BORDER_LEFT_BOTTOM" => Self::BorderLeftBottom,

            "BORDER_RIGHT_TOP" => Self::BorderRightTop,
            "BORDER_RIGHT_MIDDLE" => Self::BorderRightMiddle,
            "BORDER_RIGHT_BOTTOM" => Self::BorderRightBottom,

            "BORDER_BOTTOM_LEFT" => Self::BorderBottomLeft,
            "BORDER_BOTTOM_CENTER" => Self::BorderBottomCenter,
            "BORDER_BOTTOM_RIGHT" => Self::BorderBottomRight,

            "UNLOCKED_TOP" => Self::UnlockedTop,
            "UNLOCKED_MIDDLE" => Self::UnlockedMiddle,
            "UNLOCKED_BOTTOM" => Self::UnlockedBottom,

            _ => Self::Unset,
        }
    }

    /// Convert position to its canonical string representation.
    ///
    /// Returns `""` for [`Position::Unset`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OutsideTopLeft => "OUTSIDE_TOP_LEFT",
            Self::OutsideTopCenter => "OUTSIDE_TOP_CENTER",
            Self::OutsideTopRight => "OUTSIDE_TOP_RIGHT",

            Self::OutsideLeftTop => "OUTSIDE_LEFT_TOP",
            Self::OutsideLeftMiddle => "OUTSIDE_LEFT_MIDDLE",
            Self::OutsideLeftBottom => "OUTSIDE_LEFT_BOTTOM",

            Self::OutsideRightTop => "OUTSIDE_RIGHT_TOP",
            Self::OutsideRightMiddle => "OUTSIDE_RIGHT_MIDDLE",
            Self::OutsideRightBottom => "OUTSIDE_RIGHT_BOTTOM",

            Self::OutsideBottomLeft => "OUTSIDE_BOTTOM_LEFT",
            Self::OutsideBottomCenter => "OUTSIDE_BOTTOM_CENTER",
            Self::OutsideBottomRight => "OUTSIDE_BOTTOM_RIGHT",

            Self::InsideTopLeft => "INSIDE_TOP_LEFT",
            Self::InsideTopCenter => "INSIDE_TOP_CENTER",
            Self::InsideTopRight => "INSIDE_TOP_RIGHT",

            Self::InsideMiddleLeft => "INSIDE_MIDDLE_LEFT",
            Self::InsideMiddleCenter => "INSIDE_MIDDLE_CENTER",
            Self::InsideMiddleRight => "INSIDE_MIDDLE_RIGHT",

            Self::InsideBottomLeft => "INSIDE_BOTTOM_LEFT",
            Self::InsideBottomCenter => "INSIDE_BOTTOM_CENTER",
            Self::InsideBottomRight => "INSIDE_BOTTOM_RIGHT",

            Self::BorderTopLeft => "BORDER_TOP_LEFT",
            Self::BorderTopCenter => "BORDER_TOP_CENTER",
            Self::BorderTopRight => "BORDER_TOP_RIGHT",

            Self::BorderLeftTop => "BORDER_LEFT_TOP",
            Self::BorderLeftMiddle => "BORDER_LEFT_MIDDLE",
            Self::BorderLeftBottom => "BORDER_LEFT_BOTTOM",

            Self::BorderRightTop => "BORDER_RIGHT_TOP",
            Self::BorderRightMiddle => "BORDER_RIGHT_MIDDLE",
            Self::BorderRightBottom => "BORDER_RIGHT_BOTTOM",

            Self::BorderBottomLeft => "BORDER_BOTTOM_LEFT",
            Self::BorderBottomCenter => "BORDER_BOTTOM_CENTER",
            Self::BorderBottomRight => "BORDER_BOTTOM_RIGHT",

            Self::UnlockedTop => "UNLOCKED_TOP",
            Self::UnlockedMiddle => "UNLOCKED_MIDDLE",
            Self::UnlockedBottom => "UNLOCKED_BOTTOM",

            Self::Unset => "",
        }
    }

    /// Whether this position is valid for shapes (nodes).
    pub fn is_shape_position(&self) -> bool {
        matches!(
            self,
            Self::OutsideTopLeft
                | Self::OutsideTopCenter
                | Self::OutsideTopRight
                | Self::OutsideBottomLeft
                | Self::OutsideBottomCenter
                | Self::OutsideBottomRight
                | Self::OutsideLeftTop
                | Self::OutsideLeftMiddle
                | Self::OutsideLeftBottom
                | Self::OutsideRightTop
                | Self::OutsideRightMiddle
                | Self::OutsideRightBottom
                | Self::InsideTopLeft
                | Self::InsideTopCenter
                | Self::InsideTopRight
                | Self::InsideMiddleLeft
                | Self::InsideMiddleCenter
                | Self::InsideMiddleRight
                | Self::InsideBottomLeft
                | Self::InsideBottomCenter
                | Self::InsideBottomRight
                | Self::BorderTopLeft
                | Self::BorderTopCenter
                | Self::BorderTopRight
                | Self::BorderLeftTop
                | Self::BorderLeftMiddle
                | Self::BorderLeftBottom
                | Self::BorderRightTop
                | Self::BorderRightMiddle
                | Self::BorderRightBottom
                | Self::BorderBottomLeft
                | Self::BorderBottomCenter
                | Self::BorderBottomRight
        )
    }

    /// Whether this position is valid for edges (connections).
    pub fn is_edge_position(&self) -> bool {
        matches!(
            self,
            Self::OutsideTopLeft
                | Self::OutsideTopCenter
                | Self::OutsideTopRight
                | Self::InsideMiddleLeft
                | Self::InsideMiddleCenter
                | Self::InsideMiddleRight
                | Self::OutsideBottomLeft
                | Self::OutsideBottomCenter
                | Self::OutsideBottomRight
                | Self::UnlockedTop
                | Self::UnlockedMiddle
                | Self::UnlockedBottom
        )
    }

    /// Whether this is an outside position.
    pub fn is_outside(&self) -> bool {
        matches!(
            self,
            Self::OutsideTopLeft
                | Self::OutsideTopCenter
                | Self::OutsideTopRight
                | Self::OutsideBottomLeft
                | Self::OutsideBottomCenter
                | Self::OutsideBottomRight
                | Self::OutsideLeftTop
                | Self::OutsideLeftMiddle
                | Self::OutsideLeftBottom
                | Self::OutsideRightTop
                | Self::OutsideRightMiddle
                | Self::OutsideRightBottom
        )
    }

    /// Whether this is an unlocked position.
    pub fn is_unlocked(&self) -> bool {
        matches!(
            self,
            Self::UnlockedTop | Self::UnlockedMiddle | Self::UnlockedBottom
        )
    }

    /// Whether this is a border position.
    pub fn is_border(&self) -> bool {
        matches!(
            self,
            Self::BorderTopLeft
                | Self::BorderTopCenter
                | Self::BorderTopRight
                | Self::BorderLeftTop
                | Self::BorderLeftMiddle
                | Self::BorderLeftBottom
                | Self::BorderRightTop
                | Self::BorderRightMiddle
                | Self::BorderRightBottom
                | Self::BorderBottomLeft
                | Self::BorderBottomCenter
                | Self::BorderBottomRight
        )
    }

    /// Whether this position sits directly on the edge path.
    pub fn is_on_edge(&self) -> bool {
        matches!(
            self,
            Self::InsideMiddleLeft
                | Self::InsideMiddleCenter
                | Self::InsideMiddleRight
                | Self::UnlockedMiddle
        )
    }

    /// Return the position mirrored across both axes.
    pub fn mirrored(&self) -> Self {
        match self {
            Self::OutsideTopLeft => Self::OutsideBottomRight,
            Self::OutsideTopCenter => Self::OutsideBottomCenter,
            Self::OutsideTopRight => Self::OutsideBottomLeft,

            Self::OutsideLeftTop => Self::OutsideRightBottom,
            Self::OutsideLeftMiddle => Self::OutsideRightMiddle,
            Self::OutsideLeftBottom => Self::OutsideRightTop,

            Self::OutsideRightTop => Self::OutsideLeftBottom,
            Self::OutsideRightMiddle => Self::OutsideLeftMiddle,
            Self::OutsideRightBottom => Self::OutsideLeftTop,

            Self::OutsideBottomLeft => Self::OutsideTopRight,
            Self::OutsideBottomCenter => Self::OutsideTopCenter,
            Self::OutsideBottomRight => Self::OutsideTopLeft,

            Self::InsideTopLeft => Self::InsideBottomRight,
            Self::InsideTopCenter => Self::InsideBottomCenter,
            Self::InsideTopRight => Self::InsideBottomLeft,

            Self::InsideMiddleLeft => Self::InsideMiddleRight,
            Self::InsideMiddleCenter => Self::InsideMiddleCenter,
            Self::InsideMiddleRight => Self::InsideMiddleLeft,

            Self::InsideBottomLeft => Self::InsideTopRight,
            Self::InsideBottomCenter => Self::InsideTopCenter,
            Self::InsideBottomRight => Self::InsideTopLeft,

            Self::BorderTopLeft => Self::BorderBottomRight,
            Self::BorderTopCenter => Self::BorderBottomCenter,
            Self::BorderTopRight => Self::BorderBottomLeft,

            Self::BorderLeftTop => Self::BorderRightBottom,
            Self::BorderLeftMiddle => Self::BorderRightMiddle,
            Self::BorderLeftBottom => Self::BorderRightTop,

            Self::BorderRightTop => Self::BorderLeftBottom,
            Self::BorderRightMiddle => Self::BorderLeftMiddle,
            Self::BorderRightBottom => Self::BorderLeftTop,

            Self::BorderBottomLeft => Self::BorderTopRight,
            Self::BorderBottomCenter => Self::BorderTopCenter,
            Self::BorderBottomRight => Self::BorderTopLeft,

            Self::UnlockedTop => Self::UnlockedBottom,
            Self::UnlockedBottom => Self::UnlockedTop,
            Self::UnlockedMiddle => Self::UnlockedMiddle,

            Self::Unset => Self::Unset,
        }
    }

    /// Compute the top-left point of a label with the given `width` and `height`
    /// placed at this position on `geo_box`, with the specified `padding`.
    pub fn get_point_on_box(
        &self,
        geo_box: &crate::geo::Box,
        padding: f64,
        width: f64,
        height: f64,
    ) -> Point {
        let mut p = geo_box.top_left;
        let center = geo_box.center();

        match self {
            Self::OutsideTopLeft => {
                p.x -= padding;
                p.y -= padding + height;
            }
            Self::OutsideTopCenter => {
                p.x = center.x - width / 2.0;
                p.y -= padding + height;
            }
            Self::OutsideTopRight => {
                p.x += geo_box.width - width - padding;
                p.y -= padding + height;
            }

            Self::OutsideLeftTop => {
                p.x -= padding + width;
                p.y += padding;
            }
            Self::OutsideLeftMiddle => {
                p.x -= padding + width;
                p.y = center.y - height / 2.0;
            }
            Self::OutsideLeftBottom => {
                p.x -= padding + width;
                p.y += geo_box.height - height - padding;
            }

            Self::OutsideRightTop => {
                p.x += geo_box.width + padding;
                p.y += padding;
            }
            Self::OutsideRightMiddle => {
                p.x += geo_box.width + padding;
                p.y = center.y - height / 2.0;
            }
            Self::OutsideRightBottom => {
                p.x += geo_box.width + padding;
                p.y += geo_box.height - height - padding;
            }

            Self::OutsideBottomLeft => {
                p.x += padding;
                p.y += geo_box.height + padding;
            }
            Self::OutsideBottomCenter => {
                p.x = center.x - width / 2.0;
                p.y += geo_box.height + padding;
            }
            Self::OutsideBottomRight => {
                p.x += geo_box.width - width - padding;
                p.y += geo_box.height + padding;
            }

            Self::InsideTopLeft => {
                p.x += padding;
                p.y += padding;
            }
            Self::InsideTopCenter => {
                p.x = center.x - width / 2.0;
                p.y += padding;
            }
            Self::InsideTopRight => {
                p.x += geo_box.width - width - padding;
                p.y += padding;
            }

            Self::InsideMiddleLeft => {
                p.x += padding;
                p.y = center.y - height / 2.0;
            }
            Self::InsideMiddleCenter => {
                p.x = center.x - width / 2.0;
                p.y = center.y - height / 2.0;
            }
            Self::InsideMiddleRight => {
                p.x += geo_box.width - width - padding;
                p.y = center.y - height / 2.0;
            }

            Self::InsideBottomLeft => {
                p.x += padding;
                p.y += geo_box.height - height - padding;
            }
            Self::InsideBottomCenter => {
                p.x = center.x - width / 2.0;
                p.y += geo_box.height - height - padding;
            }
            Self::InsideBottomRight => {
                p.x += geo_box.width - width - padding;
                p.y += geo_box.height - height - padding;
            }

            Self::BorderTopLeft => {
                p.x += padding;
                p.y -= height / 2.0;
            }
            Self::BorderTopCenter => {
                p.x = center.x - width / 2.0;
                p.y -= height / 2.0;
            }
            Self::BorderTopRight => {
                p.x += geo_box.width - width - padding;
                p.y -= height / 2.0;
            }

            Self::BorderLeftTop => {
                p.x -= width / 2.0;
                p.y += padding;
            }
            Self::BorderLeftMiddle => {
                p.x -= width / 2.0;
                p.y = center.y - height / 2.0;
            }
            Self::BorderLeftBottom => {
                p.x -= width / 2.0;
                p.y += geo_box.height - height - padding;
            }

            Self::BorderRightTop => {
                p.x += geo_box.width - width / 2.0;
                p.y += padding;
            }
            Self::BorderRightMiddle => {
                p.x += geo_box.width - width / 2.0;
                p.y = center.y - height / 2.0;
            }
            Self::BorderRightBottom => {
                p.x += geo_box.width - width / 2.0;
                p.y += geo_box.height - height - padding;
            }

            Self::BorderBottomLeft => {
                p.x += padding;
                p.y += geo_box.height - height / 2.0;
            }
            Self::BorderBottomCenter => {
                p.x = center.x - width / 2.0;
                p.y += geo_box.height - height / 2.0;
            }
            Self::BorderBottomRight => {
                p.x += geo_box.width - width - padding;
                p.y += geo_box.height - height / 2.0;
            }

            Self::Unset | Self::UnlockedTop | Self::UnlockedMiddle | Self::UnlockedBottom => {}
        }

        p
    }

    /// Compute the top-left point and segment index of a label with the given
    /// `width` and `height` placed at this position on `route`.
    ///
    /// `stroke_width` is the edge stroke width used for offset calculation.
    /// `label_percentage` is used only by `Unlocked*` positions (0.0..=1.0).
    ///
    /// Returns `None` for positions that are not valid edge positions.
    pub fn get_point_on_route(
        &self,
        route: &crate::geo::Route,
        stroke_width: f64,
        label_percentage: f64,
        width: f64,
        height: f64,
    ) -> Option<(Point, usize)> {
        let total_length = route.length();
        let left_position = LEFT_LABEL_POSITION * total_length;
        let center_position = CENTER_LABEL_POSITION * total_length;
        let right_position = RIGHT_LABEL_POSITION * total_length;
        let unlocked_position = label_percentage * total_length;

        let get_offset_label_position =
            |base: &Point, norm_start: &Point, norm_end: &Point, flip: bool| -> Point {
                let (mut nx, mut ny) = crate::geo::get_unit_normal_vector(
                    norm_start.x,
                    norm_start.y,
                    norm_end.x,
                    norm_end.y,
                );
                if flip {
                    nx = -nx;
                    ny = -ny;
                }
                let offset_x = stroke_width / 2.0 + PADDING + width / 2.0;
                let offset_y = stroke_width / 2.0 + PADDING + height / 2.0;
                Point::new(base.x + nx * offset_x, base.y + ny * offset_y)
            };

        let (label_center, index) = match self {
            Self::InsideMiddleLeft => {
                let (pt, idx) = route.get_point_at_distance(left_position);
                (pt, idx)
            }
            Self::InsideMiddleCenter => {
                let (pt, idx) = route.get_point_at_distance(center_position);
                (pt, idx)
            }
            Self::InsideMiddleRight => {
                let (pt, idx) = route.get_point_at_distance(right_position);
                (pt, idx)
            }

            Self::OutsideTopLeft => {
                let (base, idx) = route.get_point_at_distance(left_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], true);
                (pt, idx)
            }
            Self::OutsideTopCenter => {
                let (base, idx) = route.get_point_at_distance(center_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], true);
                (pt, idx)
            }
            Self::OutsideTopRight => {
                let (base, idx) = route.get_point_at_distance(right_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], true);
                (pt, idx)
            }

            Self::OutsideBottomLeft => {
                let (base, idx) = route.get_point_at_distance(left_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], false);
                (pt, idx)
            }
            Self::OutsideBottomCenter => {
                let (base, idx) = route.get_point_at_distance(center_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], false);
                (pt, idx)
            }
            Self::OutsideBottomRight => {
                let (base, idx) = route.get_point_at_distance(right_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], false);
                (pt, idx)
            }

            Self::UnlockedTop => {
                let (base, idx) = route.get_point_at_distance(unlocked_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], true);
                (pt, idx)
            }
            Self::UnlockedMiddle => {
                let (pt, idx) = route.get_point_at_distance(unlocked_position);
                (pt, idx)
            }
            Self::UnlockedBottom => {
                let (base, idx) = route.get_point_at_distance(unlocked_position);
                let pt = get_offset_label_position(&base, &route[idx], &route[idx + 1], false);
                (pt, idx)
            }

            _ => return None,
        };

        // Convert from center to top-left
        let x = chop_precision(label_center.x - width / 2.0);
        let y = chop_precision(label_center.y - height / 2.0);
        Some((Point::new(x, y), index))
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Reduce floating-point precision for cross-architecture consistency.
fn chop_precision(f: f64) -> f64 {
    // Bring down to f32 precision before rounding, matching the Go implementation.
    let result = ((f as f32 * 10000.0) as f64 / 10000.0).round();
    // Ensure negative zero becomes positive zero
    if result == 0.0 { 0.0 } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All non-Unset positions for exhaustive iteration.
    const ALL_POSITIONS: &[Position] = &[
        Position::OutsideTopLeft,
        Position::OutsideTopCenter,
        Position::OutsideTopRight,
        Position::OutsideLeftTop,
        Position::OutsideLeftMiddle,
        Position::OutsideLeftBottom,
        Position::OutsideRightTop,
        Position::OutsideRightMiddle,
        Position::OutsideRightBottom,
        Position::OutsideBottomLeft,
        Position::OutsideBottomCenter,
        Position::OutsideBottomRight,
        Position::InsideTopLeft,
        Position::InsideTopCenter,
        Position::InsideTopRight,
        Position::InsideMiddleLeft,
        Position::InsideMiddleCenter,
        Position::InsideMiddleRight,
        Position::InsideBottomLeft,
        Position::InsideBottomCenter,
        Position::InsideBottomRight,
        Position::BorderTopLeft,
        Position::BorderTopCenter,
        Position::BorderTopRight,
        Position::BorderLeftTop,
        Position::BorderLeftMiddle,
        Position::BorderLeftBottom,
        Position::BorderRightTop,
        Position::BorderRightMiddle,
        Position::BorderRightBottom,
        Position::BorderBottomLeft,
        Position::BorderBottomCenter,
        Position::BorderBottomRight,
        Position::UnlockedTop,
        Position::UnlockedMiddle,
        Position::UnlockedBottom,
    ];

    #[test]
    fn string_round_trip() {
        for &pos in ALL_POSITIONS {
            let s = pos.as_str();
            assert!(
                !s.is_empty(),
                "non-Unset position should have a string: {:?}",
                pos
            );
            let parsed = Position::from_string(s);
            assert_eq!(parsed, pos, "round-trip failed for {s}");
        }
    }

    #[test]
    fn from_string_unknown_returns_unset() {
        assert_eq!(Position::from_string("BOGUS"), Position::Unset);
        assert_eq!(Position::from_string(""), Position::Unset);
    }

    #[test]
    fn unset_string_is_empty() {
        assert_eq!(Position::Unset.as_str(), "");
    }

    #[test]
    fn display_matches_as_str() {
        for &pos in ALL_POSITIONS {
            assert_eq!(format!("{pos}"), pos.as_str());
        }
    }

    #[test]
    fn mirrored_is_involution() {
        for &pos in ALL_POSITIONS {
            assert_eq!(
                pos.mirrored().mirrored(),
                pos,
                "mirrored should be an involution for {:?}",
                pos,
            );
        }
    }

    #[test]
    fn mirrored_preserves_category() {
        for &pos in ALL_POSITIONS {
            let m = pos.mirrored();
            assert_eq!(pos.is_outside(), m.is_outside());
            assert_eq!(pos.is_border(), m.is_border());
            assert_eq!(pos.is_unlocked(), m.is_unlocked());
        }
    }

    #[test]
    fn classification_coverage() {
        // Every non-Unset position should be in at least one category
        for &pos in ALL_POSITIONS {
            let any = pos.is_outside()
                || pos.is_border()
                || pos.is_unlocked()
                || matches!(
                    pos,
                    Position::InsideTopLeft
                        | Position::InsideTopCenter
                        | Position::InsideTopRight
                        | Position::InsideMiddleLeft
                        | Position::InsideMiddleCenter
                        | Position::InsideMiddleRight
                        | Position::InsideBottomLeft
                        | Position::InsideBottomCenter
                        | Position::InsideBottomRight
                );
            assert!(any, "{:?} is not in any category", pos);
        }
    }

    #[test]
    fn shape_positions_exclude_unlocked() {
        assert!(!Position::UnlockedTop.is_shape_position());
        assert!(!Position::UnlockedMiddle.is_shape_position());
        assert!(!Position::UnlockedBottom.is_shape_position());
        assert!(!Position::Unset.is_shape_position());
    }

    #[test]
    fn edge_positions_include_unlocked() {
        assert!(Position::UnlockedTop.is_edge_position());
        assert!(Position::UnlockedMiddle.is_edge_position());
        assert!(Position::UnlockedBottom.is_edge_position());
    }

    #[test]
    fn default_is_unset() {
        assert_eq!(Position::default(), Position::Unset);
    }

    #[test]
    fn get_point_on_box_inside_middle_center() {
        let b = crate::geo::Box::new(Point::new(0.0, 0.0), 100.0, 80.0);
        let pt = Position::InsideMiddleCenter.get_point_on_box(&b, PADDING, 20.0, 10.0);
        // center of box is (50, 40), label center should be placed there
        assert!((pt.x - 40.0).abs() < 1e-9, "x={}", pt.x);
        assert!((pt.y - 35.0).abs() < 1e-9, "y={}", pt.y);
    }

    #[test]
    fn get_point_on_box_outside_top_center() {
        let b = crate::geo::Box::new(Point::new(10.0, 20.0), 100.0, 80.0);
        let pt = Position::OutsideTopCenter.get_point_on_box(&b, 5.0, 30.0, 12.0);
        // center.x = 60, so label x = 60 - 15 = 45
        // y = 20 - 5 - 12 = 3
        assert!((pt.x - 45.0).abs() < 1e-9, "x={}", pt.x);
        assert!((pt.y - 3.0).abs() < 1e-9, "y={}", pt.y);
    }

    #[test]
    fn get_point_on_route_inside_middle_center() {
        // Horizontal route from (0,0) to (100,0)
        let route = crate::geo::Route(vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]);
        let result = Position::InsideMiddleCenter.get_point_on_route(&route, 2.0, 0.0, 20.0, 10.0);
        assert!(result.is_some());
        let (pt, idx) = result.unwrap();
        assert_eq!(idx, 0);
        // center position = 50% of 100 = 50, label top-left = (50-10, 0-5) = (40, -5)
        assert!((pt.x - 40.0).abs() < 1e-2, "x={}", pt.x);
        assert!((pt.y - (-5.0)).abs() < 1e-2, "y={}", pt.y);
    }

    #[test]
    fn get_point_on_route_returns_none_for_non_edge() {
        let route = crate::geo::Route(vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]);
        assert!(
            Position::InsideTopLeft
                .get_point_on_route(&route, 2.0, 0.0, 20.0, 10.0)
                .is_none()
        );
        assert!(
            Position::Unset
                .get_point_on_route(&route, 2.0, 0.0, 20.0, 10.0)
                .is_none()
        );
    }

    #[test]
    fn chop_precision_negative_zero() {
        assert_eq!(chop_precision(-0.0), 0.0);
        assert!(chop_precision(-0.0).is_sign_positive());
    }
}
