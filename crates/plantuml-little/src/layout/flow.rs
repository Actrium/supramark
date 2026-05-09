use std::collections::HashMap;

use crate::font_metrics;
use crate::model::flow::{FlowDiagram, FlowDirection};
use crate::{Error, Result};

const SINGLE_SIZE_X: f64 = 100.0;
const SINGLE_SIZE_Y: f64 = 35.0;
const BOX_MARGIN: f64 = 10.0;
#[allow(dead_code)] // Java-ported layout constant
const CORNER_RADIUS: f64 = 25.0;
const FONT_SIZE: f64 = 14.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlowPosition {
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
}

impl FlowPosition {
    fn root() -> Self {
        Self {
            xmin: 0,
            ymin: 0,
            xmax: 1,
            ymax: 1,
        }
    }

    fn move_dir(self, direction: FlowDirection, delta: i32) -> Self {
        match direction {
            FlowDirection::North => Self {
                xmin: self.xmin,
                ymin: self.ymin - delta,
                xmax: self.xmax,
                ymax: self.ymax - delta,
            },
            FlowDirection::South => Self {
                xmin: self.xmin,
                ymin: self.ymin + delta,
                xmax: self.xmax,
                ymax: self.ymax + delta,
            },
            FlowDirection::East => Self {
                xmin: self.xmin + delta,
                ymin: self.ymin,
                xmax: self.xmax + delta,
                ymax: self.ymax,
            },
            FlowDirection::West => Self {
                xmin: self.xmin - delta,
                ymin: self.ymin,
                xmax: self.xmax - delta,
                ymax: self.ymax,
            },
        }
    }

    fn center_x(self) -> i32 {
        (self.xmin + self.xmax + 1) / 2
    }

    fn center_y(self) -> i32 {
        (self.ymin + self.ymax + 1) / 2
    }
}

#[derive(Debug, Clone)]
pub struct FlowNodeLayout {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text_x: f64,
    pub text_y: f64,
    pub text_length: f64,
}

#[derive(Debug, Clone)]
pub struct FlowPathLayout {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub ellipse_cx: f64,
    pub ellipse_cy: f64,
}

#[derive(Debug, Clone)]
pub struct FlowLayout {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<FlowNodeLayout>,
    pub paths: Vec<FlowPathLayout>,
}

pub fn layout_flow(diagram: &FlowDiagram) -> Result<FlowLayout> {
    let mut positions: HashMap<String, FlowPosition> = HashMap::new();
    let mut last_id: Option<String> = None;

    for node in &diagram.nodes {
        let pos = if let Some(direction) = node.placement {
            let last = last_id
                .as_ref()
                .and_then(|id| positions.get(id).copied())
                .ok_or_else(|| Error::Layout("flow node missing previous position".into()))?;
            let mut candidate = last.move_dir(direction, 2);
            while positions.values().any(|existing| *existing == candidate) {
                move_all_to_east(&mut positions, candidate);
                candidate = last.move_dir(direction, 2);
            }
            candidate
        } else {
            FlowPosition::root()
        };
        positions.insert(node.id.clone(), pos);
        last_id = Some(node.id.clone());
    }

    let min_x = positions.values().map(|p| p.xmin).min().unwrap_or(0);
    let min_y = positions.values().map(|p| p.ymin).min().unwrap_or(0);
    let max_x = positions.values().map(|p| p.xmax).max().unwrap_or(1);
    let max_y = positions.values().map(|p| p.ymax).max().unwrap_or(1);

    let origin_x = -(min_x as f64) * SINGLE_SIZE_X;
    let origin_y = -(min_y as f64) * SINGLE_SIZE_Y;

    let mut nodes = Vec::with_capacity(diagram.nodes.len());
    let mut node_map: HashMap<&str, usize> = HashMap::new();
    for node in &diagram.nodes {
        let pos = positions[&node.id];
        let text_length = serif_text_width(&node.label, FONT_SIZE);
        let width = text_length + 2.0 * BOX_MARGIN;
        let height = serif_line_height(FONT_SIZE) + 2.0 * BOX_MARGIN;
        let delta_x = SINGLE_SIZE_X * 2.0 - width;
        let delta_y = SINGLE_SIZE_Y * 2.0 - height;
        let x = origin_x + (pos.xmin as f64) * SINGLE_SIZE_X + delta_x / 2.0;
        let y = origin_y + (pos.ymin as f64) * SINGLE_SIZE_Y + delta_y / 2.0;
        let idx = nodes.len();
        node_map.insert(&node.id, idx);
        nodes.push(FlowNodeLayout {
            id: node.id.clone(),
            label: node.label.clone(),
            x,
            y,
            width,
            height,
            text_x: x + BOX_MARGIN,
            text_y: y + BOX_MARGIN + serif_ascent(FONT_SIZE),
            text_length,
        });
    }

    let mut paths = Vec::with_capacity(diagram.links.len());
    for link in &diagram.links {
        let from_idx = *node_map
            .get(link.from.as_str())
            .ok_or_else(|| Error::Layout(format!("unknown flow node {}", link.from)))?;
        let to_idx = *node_map
            .get(link.to.as_str())
            .ok_or_else(|| Error::Layout(format!("unknown flow node {}", link.to)))?;
        let from_pos = positions[&link.from];
        let to_pos = positions[&link.to];
        validate_path(from_pos, link.direction, to_pos, link.direction.opposite())?;

        let start_center = (
            origin_x + (from_pos.center_x() as f64) * SINGLE_SIZE_X,
            origin_y + (from_pos.center_y() as f64) * SINGLE_SIZE_Y,
        );
        let dest_center = (
            origin_x + (to_pos.center_x() as f64) * SINGLE_SIZE_X,
            origin_y + (to_pos.center_y() as f64) * SINGLE_SIZE_Y,
        );
        let from_node = &nodes[from_idx];
        let to_node = &nodes[to_idx];
        let (x1, y1) = move_point(
            start_center,
            from_node.width,
            from_node.height,
            link.direction,
        );
        let (x2, y2) = move_point(
            dest_center,
            to_node.width,
            to_node.height,
            link.direction.opposite(),
        );
        paths.push(FlowPathLayout {
            x1,
            y1,
            x2,
            y2,
            ellipse_cx: x2 + 0.5,
            ellipse_cy: y2 + 0.5,
        });
    }

    Ok(FlowLayout {
        width: ((max_x - min_x + 1) as f64) * SINGLE_SIZE_X,
        height: ((max_y - min_y + 1) as f64) * SINGLE_SIZE_Y,
        nodes,
        paths,
    })
}

fn move_all_to_east(positions: &mut HashMap<String, FlowPosition>, starting: FlowPosition) {
    for pos in positions.values_mut() {
        if pos.xmax < starting.xmin || pos.ymax < starting.ymin {
            continue;
        }
        *pos = pos.move_dir(FlowDirection::East, 2);
    }
}

fn validate_path(
    start: FlowPosition,
    start_dir: FlowDirection,
    dest: FlowPosition,
    dest_dir: FlowDirection,
) -> Result<()> {
    if start == dest {
        if start_dir == dest_dir {
            return Err(Error::Layout("invalid self path in flow diagram".into()));
        }
        return Ok(());
    }
    if start_dir == opposite(dest_dir) {
        let adjoining = match start_dir {
            FlowDirection::East => {
                start.ymin == dest.ymin && start.ymax == dest.ymax && start.xmax + 1 == dest.xmin
            }
            FlowDirection::West => {
                start.ymin == dest.ymin && start.ymax == dest.ymax && start.xmin == dest.xmax + 1
            }
            FlowDirection::South => {
                start.xmin == dest.xmin && start.xmax == dest.xmax && start.ymax + 1 == dest.ymin
            }
            FlowDirection::North => {
                start.xmin == dest.xmin && start.xmax == dest.xmax && start.ymin == dest.ymax + 1
            }
        };
        if adjoining {
            return Ok(());
        }
    }
    if start.ymin == dest.ymin
        && start.ymax == dest.ymax
        && start_dir == FlowDirection::West
        && dest_dir == FlowDirection::East
    {
        return Ok(());
    }
    Err(Error::Layout("unsupported flow path geometry".into()))
}

fn opposite(direction: FlowDirection) -> FlowDirection {
    direction.opposite()
}

fn move_point(center: (f64, f64), width: f64, height: f64, direction: FlowDirection) -> (f64, f64) {
    match direction {
        FlowDirection::South => (center.0, center.1 + height / 2.0),
        FlowDirection::North => (center.0, center.1 - height / 2.0),
        FlowDirection::East => (center.0 + width / 2.0, center.1),
        FlowDirection::West => (center.0 - width / 2.0, center.1),
    }
}

// These helpers historically reached into `crate::font_data::DEJAVU_SERIF`
// directly to compute glyph_advance / ascender / descender / units_per_em.
// They now route through the same `Metrics` trait the rest of the crate
// uses, which dispatches to the impl picked by the `metrics-*` Cargo
// features. With the default `metrics-static-dejavu` the output is
// byte-identical to the previous direct table lookups (the static impl
// applies the same `advance / units_per_em * size` formula).

fn serif_text_width(text: &str, size: f64) -> f64 {
    font_metrics::text_width(text, "Serif", size, false, false)
}

fn serif_ascent(size: f64) -> f64 {
    font_metrics::ascent("Serif", size, false, false)
}

fn serif_line_height(size: f64) -> f64 {
    font_metrics::line_height("Serif", size, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::flow::{FlowLink, FlowNode};

    #[test]
    fn lays_out_vertical_loopback() {
        let diagram = FlowDiagram {
            nodes: vec![
                FlowNode {
                    id: "one".into(),
                    label: "Start".into(),
                    placement: None,
                },
                FlowNode {
                    id: "two".into(),
                    label: "Second".into(),
                    placement: Some(FlowDirection::South),
                },
            ],
            links: vec![
                FlowLink {
                    from: "one".into(),
                    to: "two".into(),
                    direction: FlowDirection::South,
                },
                FlowLink {
                    from: "two".into(),
                    to: "one".into(),
                    direction: FlowDirection::North,
                },
            ],
        };
        let layout = layout_flow(&diagram).unwrap();
        assert_eq!(layout.width, 200.0);
        assert_eq!(layout.height, 140.0);
        assert_eq!(layout.paths.len(), 2);
    }
}
