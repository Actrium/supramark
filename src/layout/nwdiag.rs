use std::collections::HashMap;

use log::debug;

use crate::font_metrics;
use crate::model::nwdiag::NwdiagDiagram;
use crate::Result;

/// Server-name → list of (network_index, address).
type ConnectionMap = (Vec<String>, HashMap<String, Vec<(usize, String)>>);

// ---------------------------------------------------------------------------
// Public layout result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NwdiagLayout {
    /// Height reserved above the body for the title (handled by wrap_with_meta).
    pub title_height: f64,
    /// Network label blocks (drawn to the left of the grid).
    pub net_labels: Vec<NetLabelLayout>,
    /// Network tubes (thin horizontal rects).
    pub tubes: Vec<TubeLayout>,
    /// Per-server link groups (links + address labels in Java rendering order).
    pub server_link_groups: Vec<ServerLinkGroup>,
    /// Server boxes (rectangles with text label).
    pub server_boxes: Vec<ServerBoxLayout>,
    /// Total SVG body dimensions (before wrap_with_meta).
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
pub struct NetLabelLayout {
    /// Network display name.
    pub name: String,
    /// Network address (optional, displayed below name).
    pub address: Option<String>,
    /// X position of the text (right edge, since text is left-aligned in Java reference).
    pub x: f64,
    /// Y baseline for the name text.
    pub y: f64,
    /// Y baseline for the address text (only if address present).
    pub addr_y: f64,
}

#[derive(Debug, Clone)]
pub struct TubeLayout {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LinkLayout {
    pub x: f64,
    pub y1: f64,
    pub y2: f64,
}

#[derive(Debug, Clone)]
pub struct AddrLabelLayout {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

/// A group of links and address labels for a single server,
/// emitted in the Java rendering order (top link, top addr, bottom links+addrs).
#[derive(Debug, Clone)]
pub struct ServerLinkGroup {
    pub links_and_labels: Vec<LinkOrLabel>,
}

#[derive(Debug, Clone)]
pub enum LinkOrLabel {
    Link(LinkLayout),
    Label(AddrLabelLayout),
}

#[derive(Debug, Clone)]
pub struct ServerBoxLayout {
    pub label: String,
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    /// X position for the text (left edge; text is left-aligned in Java).
    pub text_x: f64,
    /// Y baseline for the text.
    pub text_y: f64,
}

// ---------------------------------------------------------------------------
// Constants matching Java PlantUML
// ---------------------------------------------------------------------------

/// Java NwDiagram.margin = 5
const MARGIN: f64 = 5.0;

/// Java GridTextBlockDecorated.NETWORK_THIN = 5
const NETWORK_THIN: f64 = 5.0;

/// Java NServerDraw.MAGIC = 15
const MAGIC: f64 = 15.0;

/// Java NServerDraw.marginAd = 10
const MARGIN_AD: f64 = 10.0;

/// Java NServerDraw.marginBoxW() = 15
const MARGIN_BOX_W: f64 = 15.0;

/// Java GridTextBlockSimple.lineHeight() minimum = 50
const MIN_LINE_HEIGHT: f64 = 50.0;

/// Java GridTextBlockSimple.MINIMUM_WIDTH = 70
#[allow(dead_code)]
const MINIMUM_WIDTH: f64 = 70.0;

/// Font size for network labels and server box text.
const FONT_SIZE_12: f64 = 12.0;

/// Font size for address labels on connecting lines.
const FONT_SIZE_11: f64 = 11.0;

/// USymbol.RECTANGLE padding: 10px each side.
const BOX_PAD: f64 = 10.0;

/// Gap between network labels and the grid (Java: deltaX += 5).
const LABEL_GRID_GAP: f64 = 5.0;

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the display label for a server.
///
/// Java: NServer stores `description` which defaults to `name` and is
/// overwritten by the `description` property in each network declaration.
/// The LAST declaration's `description` wins.
fn resolve_description(name: &str, diagram: &NwdiagDiagram) -> String {
    let mut desc = name.to_string();
    for net in &diagram.networks {
        for srv in &net.servers {
            if srv.name == name {
                if let Some(d) = &srv.description {
                    desc = d.clone();
                }
            }
        }
    }
    desc
}

/// Build per-server connection map: server_name -> Vec<(network_index, address)>.
/// The first entry is the "main network" (first network the server appears in).
fn build_connections(diagram: &NwdiagDiagram) -> ConnectionMap {
    let mut server_order: Vec<String> = Vec::new();
    let mut conns: HashMap<String, Vec<(usize, String)>> = HashMap::new();

    for (net_idx, net) in diagram.networks.iter().enumerate() {
        for srv in &net.servers {
            if !conns.contains_key(&srv.name) {
                server_order.push(srv.name.clone());
            }
            let addr = srv.address.clone().unwrap_or_default();
            conns
                .entry(srv.name.clone())
                .or_default()
                .push((net_idx, addr));
        }
    }

    (server_order, conns)
}

/// Tetris column assignment.
///
/// Each server has a "bar" spanning NStages. The stages model is:
/// - Each network N has nstage = N (0-based).
/// - Each network N (except the first) also has up = N-1 (the stage before it).
/// - When a server first connects to network N, bar gets stage N.
/// - When a server subsequently connects to another network M, bar gets
///   stage M.up = M-1 (NOT M itself).
///
/// Servers are placed in the first column where their bar's stages are all free.
fn tetris_layout(
    server_order: &[String],
    conns: &HashMap<String, Vec<(usize, String)>>,
    num_networks: usize,
) -> HashMap<String, usize> {
    struct Bar {
        name: String,
        start: usize,
        end: usize,
    }

    let bars: Vec<Bar> = server_order
        .iter()
        .filter_map(|name| {
            let c = conns.get(name)?;
            if c.is_empty() {
                return None;
            }
            let main_net = c[0].0;
            // First connection: bar gets main_net's nstage.
            let mut start = main_net;
            let mut end = main_net;
            // Subsequent connections: bar gets network.up = net_idx - 1 (but min 0).
            for &(net_idx, _) in c.iter().skip(1) {
                let up = if net_idx > 0 { net_idx - 1 } else { 0 };
                start = start.min(up);
                end = end.max(up);
            }
            Some(Bar {
                name: name.clone(),
                start,
                end,
            })
        })
        .collect();

    // Total number of stages = num_networks (stages 0..num_networks-1).
    let num_stages = num_networks;

    // Grid of occupied cells: grid[stage][col] = occupied?
    let mut grid: Vec<Vec<bool>> = vec![vec![false; bars.len().max(1)]; num_stages];
    let mut result = HashMap::new();

    for bar in &bars {
        let mut col = 0;
        loop {
            // Ensure grid columns are wide enough.
            for row in grid.iter_mut() {
                if row.len() <= col {
                    row.resize(col + 1, false);
                }
            }
            // Check if all stages in [start, end] are free at this column.
            let fits = (bar.start..=bar.end).all(|stage| !grid[stage][col]);
            if fits {
                #[allow(clippy::needless_range_loop)]
                for stage in bar.start..=bar.end {
                    grid[stage][col] = true;
                }
                result.insert(bar.name.clone(), col);
                break;
            }
            col += 1;
            if col > 100 {
                result.insert(bar.name.clone(), col);
                break;
            }
        }
    }

    result
}

/// Text width for sans-serif at given size.
fn tw(text: &str, size: f64) -> f64 {
    font_metrics::text_width(text, "SansSerif", size, false, false)
}

/// Line height for sans-serif at given size.
fn lh(size: f64) -> f64 {
    font_metrics::line_height("SansSerif", size, false, false)
}

/// Font ascent for sans-serif at given size.
fn fa(size: f64) -> f64 {
    font_metrics::ascent("SansSerif", size, false, false)
}

/// Compute server box dimensions (USymbol.RECTANGLE with padding=10).
fn box_dim(label: &str, font_size: f64) -> (f64, f64) {
    let text_w = tw(label, font_size);
    let text_h = lh(font_size);
    (text_w + 2.0 * BOX_PAD, text_h + 2.0 * BOX_PAD)
}

/// NServerDraw.naturalDimension equivalent.
///
/// Returns (width, height) of a grid cell for a given server.
fn natural_dimension(
    description: &str,
    main_addr: &str,
    next_net_addr: Option<&str>,
    _num_networks: usize,
) -> (f64, f64) {
    let (box_w, box_h) = box_dim(description, FONT_SIZE_12);
    let top_margin = MAGIC; // marginBoxH = topMargin = MAGIC

    // link1: address on the main network connection
    let (link1_w, link1_h) = if main_addr.is_empty() {
        (0.0, 0.0)
    } else {
        (tw(main_addr, FONT_SIZE_11), lh(FONT_SIZE_11))
    };

    // link2: address on the next network connection (if any)
    let (link2_w, link2_h) = match next_net_addr {
        Some(addr) if !addr.is_empty() => (tw(addr, FONT_SIZE_11), lh(FONT_SIZE_11)),
        _ => (0.0, 0.0),
    };

    let width = f64::max(
        link1_w + 2.0 * MARGIN_AD,
        f64::max(box_w + 2.0 * MARGIN_BOX_W, link2_w + 2.0 * MARGIN_AD),
    );
    let height = link1_h + 2.0 * MARGIN_AD + 2.0 * top_margin + box_h + link2_h + 2.0 * MARGIN_AD;

    (width, height)
}

// ---------------------------------------------------------------------------
// Main layout function
// ---------------------------------------------------------------------------

pub fn layout_nwdiag(diagram: &NwdiagDiagram) -> Result<NwdiagLayout> {
    let num_networks = diagram.networks.len();
    if num_networks == 0 {
        return Ok(NwdiagLayout {
            title_height: 0.0,
            net_labels: Vec::new(),
            tubes: Vec::new(),
            server_link_groups: Vec::new(),
            server_boxes: Vec::new(),
            width: 10.0,
            height: 10.0,
        });
    }

    let (server_order, conns) = build_connections(diagram);
    let col_assign = tetris_layout(&server_order, &conns, num_networks);
    let num_cols = col_assign.values().copied().max().map_or(0, |m| m + 1);

    debug!(
        "nwdiag layout: {} networks, {} servers, {} columns",
        num_networks,
        server_order.len(),
        num_cols
    );

    // Resolve descriptions for all servers.
    let descriptions: HashMap<String, String> = server_order
        .iter()
        .map(|name| (name.clone(), resolve_description(name, diagram)))
        .collect();

    // Build grid of naturalDimensions: grid[row][col] = Option<(w, h)>
    // Each server is placed in row = main_network_index, col = tetris column.
    let mut grid: Vec<Vec<Option<(f64, f64)>>> = vec![vec![None; num_cols]; num_networks];
    let mut grid_servers: Vec<Vec<Option<String>>> = vec![vec![None; num_cols]; num_networks];

    for name in &server_order {
        let c = &conns[name];
        let main_net_idx = c[0].0;
        let main_addr = &c[0].1;
        let col = col_assign[name];
        let desc = &descriptions[name];

        // link2: address on the next network (networks[main_net_idx + 1]) if server is connected.
        let next_net_addr = if main_net_idx + 1 < num_networks {
            c.iter()
                .find(|(idx, _)| *idx == main_net_idx + 1)
                .map(|(_, addr)| addr.as_str())
        } else {
            None
        };

        let dim = natural_dimension(desc, main_addr, next_net_addr, num_networks);
        grid[main_net_idx][col] = Some(dim);
        grid_servers[main_net_idx][col] = Some(name.clone());
    }

    // Compute column widths and row heights.
    let mut col_widths = vec![0.0_f64; num_cols];
    #[allow(clippy::needless_range_loop)]
    for j in 0..num_cols {
        for i in 0..num_networks {
            if let Some((w, _)) = grid[i][j] {
                col_widths[j] = col_widths[j].max(w);
            }
        }
    }

    let mut row_heights = vec![MIN_LINE_HEIGHT; num_networks];
    #[allow(clippy::needless_range_loop)]
    for i in 0..num_networks {
        for j in 0..num_cols {
            if let Some((_, h)) = grid[i][j] {
                row_heights[i] = row_heights[i].max(h);
            }
        }
    }

    // Compute network label dimensions.
    // Java: toTextBlockForNetworkName renders "name\naddress" with font from style.
    // The text is right-aligned, drawn to the left of the grid.
    let line_h_12 = lh(FONT_SIZE_12);
    let mut net_label_widths = Vec::new();
    let mut net_label_heights = Vec::new();
    for net in &diagram.networks {
        let name_w = tw(&net.name, FONT_SIZE_12);
        let addr_w = net
            .address
            .as_ref()
            .map(|a| tw(a, FONT_SIZE_12))
            .unwrap_or(0.0);
        let w = name_w.max(addr_w);
        let h = if net.address.is_some() {
            2.0 * line_h_12
        } else {
            line_h_12
        };
        net_label_widths.push(w);
        net_label_heights.push(h);
    }

    let delta_x = net_label_widths.iter().copied().fold(0.0_f64, f64::max);
    let delta_y = (net_label_heights[0] - NETWORK_THIN) / 2.0;

    // Grid origin (in body-local coords, after margin translate).
    let grid_x = delta_x + LABEL_GRID_GAP;
    let grid_y = delta_y;

    // Now compute absolute positions (body-local, with margin=5 offset baked in).
    let mx = MARGIN;
    let my = MARGIN;

    // --- Network labels ---
    let mut net_labels = Vec::new();
    let ascent_12 = fa(FONT_SIZE_12);
    let mut y_acc = 0.0;
    for (i, net) in diagram.networks.iter().enumerate() {
        let name_w = tw(&net.name, FONT_SIZE_12);
        let _addr_w = net
            .address
            .as_ref()
            .map(|a| tw(a, FONT_SIZE_12))
            .unwrap_or(0.0);

        // Java draws labels right-aligned: desc.drawU at (deltaX - dim.getWidth(), y).
        // For our multi-line block, the name and address are separate lines.
        // name x = deltaX - name_w, name y = y_acc + ascent
        // addr x = deltaX - addr_w, addr y = y_acc + line_h_12 + ascent
        let name_x = mx + delta_x - name_w;
        let name_y = my + y_acc + ascent_12;
        let addr_y = my + y_acc + line_h_12 + ascent_12;

        net_labels.push(NetLabelLayout {
            name: net.name.clone(),
            address: net.address.clone(),
            x: name_x,
            y: name_y,
            addr_y,
        });

        // Also store the address x for separate rendering.
        // We'll store the address x in the label layout... but the struct doesn't have it.
        // Let me adjust.

        y_acc += row_heights[i];
    }

    // --- Network tubes ---
    let mut tubes = Vec::new();
    y_acc = 0.0;
    for (i, net) in diagram.networks.iter().enumerate() {
        // Compute tube xmin/xmax from connected columns.
        let mut xmin = -1.0_f64;
        let mut xmax = 0.0_f64;
        let mut x = 0.0;
        #[allow(clippy::needless_range_loop)]
        for j in 0..num_cols {
            let is_linked = is_server_linked_to_network(j, i, &grid_servers, &conns);
            if is_linked && xmin < 0.0 {
                xmin = x;
            }
            x += col_widths[j];
            if is_linked {
                xmax = x;
            }
        }
        if xmin < 0.0 {
            xmin = 0.0;
        }

        let tube_width = (xmax - xmin).max(MINIMUM_WIDTH);
        let tube_x = mx + grid_x + xmin;
        let tube_y = my + grid_y + y_acc;

        tubes.push(TubeLayout {
            x: tube_x,
            y: tube_y,
            width: tube_width,
            height: NETWORK_THIN,
            color: net.color.clone(),
        });

        y_acc += row_heights[i];
    }

    // --- Server boxes and links ---
    let mut server_link_groups = Vec::new();
    let mut server_boxes = Vec::new();

    for name in &server_order {
        let c = &conns[name];
        let main_net_idx = c[0].0;
        let main_addr = &c[0].1;
        let col = col_assign[name];
        let desc = &descriptions[name];

        // Cell coordinates in the grid.
        let cell_x: f64 = col_widths[..col].iter().sum();
        let cell_y: f64 = row_heights[..main_net_idx].iter().sum();
        let cell_w = col_widths[col];
        let cell_h = row_heights[main_net_idx];

        // Absolute cell origin.
        let abs_cell_x = mx + grid_x + cell_x;
        let abs_cell_y = my + grid_y + cell_y;

        // Server box dimensions.
        let (bw, bh) = box_dim(desc, FONT_SIZE_12);
        let x_middle = cell_w / 2.0;
        let y_middle = cell_h / 2.0;

        // Box position (centered in cell).
        let box_x = abs_cell_x + x_middle - bw / 2.0;
        let box_y = abs_cell_y + y_middle - bh / 2.0;

        // Text position inside box (left-aligned with BOX_PAD margin).
        let text_x = box_x + BOX_PAD;
        let text_y = box_y + BOX_PAD + ascent_12;

        server_boxes.push(ServerBoxLayout {
            label: desc.clone(),
            rect_x: box_x,
            rect_y: box_y,
            rect_w: bw,
            rect_h: bh,
            text_x,
            text_y,
        });

        // --- Links (per-server group) ---
        let mut group_items = Vec::new();

        // Tube y for main network.
        let tube_y_main = tubes[main_net_idx].y;
        let top_margin = MAGIC;

        // Alpha = distance from cell top to box top.
        let alpha = y_middle - bh / 2.0;

        // Network stage parity for magicDelta.
        let magic_delta_main = magic_delta(main_net_idx);

        // Top link: from tube bottom to box top.
        let link_x = abs_cell_x + x_middle + magic_delta_main;
        let link_y1 = tube_y_main + NETWORK_THIN;
        let link_y2 = abs_cell_y + alpha;

        if link_y2 > link_y1 {
            group_items.push(LinkOrLabel::Link(LinkLayout {
                x: link_x,
                y1: link_y1,
                y2: link_y2,
            }));
        }

        // Address label on top link (immediately after top link, matching Java order).
        if !main_addr.is_empty() {
            let pos_link1 = (y_middle - bh / 2.0 - top_margin + MAGIC) / 2.0;
            let addr_text_w = tw(main_addr, FONT_SIZE_11);
            let addr_text_h = lh(FONT_SIZE_11);
            let addr_cx = abs_cell_x + x_middle + magic_delta_main;
            let addr_cy = abs_cell_y + pos_link1;
            group_items.push(LinkOrLabel::Label(AddrLabelLayout {
                text: main_addr.clone(),
                x: addr_cx - addr_text_w / 2.0,
                y: addr_cy - addr_text_h / 2.0 + fa(FONT_SIZE_11),
            }));
        }

        // Bottom links: to other networks this server is connected to.
        for &(net_idx, ref addr) in c.iter().skip(1) {
            let tube_y_other = tubes[net_idx].y;
            let magic_delta_other = magic_delta(net_idx);

            // Link from box bottom to other network tube.
            let link_x2 = abs_cell_x + x_middle - magic_delta_other;
            let link_y1_bottom = abs_cell_y + y_middle + bh / 2.0;
            let link_y2_bottom = tube_y_other;

            if link_y2_bottom > link_y1_bottom {
                group_items.push(LinkOrLabel::Link(LinkLayout {
                    x: link_x2,
                    y1: link_y1_bottom,
                    y2: link_y2_bottom,
                }));
            }

            // Address label on bottom link.
            if !addr.is_empty() {
                let addr_text_w = tw(addr, FONT_SIZE_11);
                let addr_text_h = lh(FONT_SIZE_11);
                let addr_cx = link_x2;
                let addr_cy = tube_y_other - alpha / 2.0;
                group_items.push(LinkOrLabel::Label(AddrLabelLayout {
                    text: addr.clone(),
                    x: addr_cx - addr_text_w / 2.0,
                    y: addr_cy - addr_text_h / 2.0 + fa(FONT_SIZE_11),
                }));
            }
        }

        server_link_groups.push(ServerLinkGroup {
            links_and_labels: group_items,
        });
    }

    // --- Total dimensions ---
    // Java: grid.calculateDimension + deltaX + margin + 1 (UEmpty at end).
    let grid_w: f64 = col_widths.iter().sum::<f64>().max(MINIMUM_WIDTH);
    let grid_h: f64 = row_heights.iter().sum();

    // Java drawMe: ug.draw(UEmpty(1,1)) at (gridW + deltaX + margin, gridH + deltaY + margin)
    // from the body origin (which is at margin,margin from SVG origin).
    // The LimitFinder maxX is the body origin + UEmpty(1,1), i.e. one px past
    // the right margin. The remaining canvas growth (Java AWT metrics being
    // slightly wider than DejaVu) is absorbed by `get_final_dim_extra` in
    // `wrap_with_meta`, leaving body_w usable for centring meta blocks.
    let body_w = MARGIN + grid_w + grid_x + MARGIN + 1.0;
    let body_h = MARGIN + grid_h + grid_y + MARGIN + 1.0;

    Ok(NwdiagLayout {
        title_height: 0.0,
        net_labels,
        tubes,
        server_link_groups,
        server_boxes,
        width: body_w,
        height: body_h,
    })
}

/// Check if any server in the given column is linked to the given network.
fn is_server_linked_to_network(
    col: usize,
    net_idx: usize,
    grid_servers: &[Vec<Option<String>>],
    conns: &HashMap<String, Vec<(usize, String)>>,
) -> bool {
    for row in grid_servers {
        if col < row.len() {
            if let Some(name) = &row[col] {
                if let Some(c) = conns.get(name) {
                    if c.iter().any(|(idx, _)| *idx == net_idx) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Java Network.magicDelta(): returns ±2 based on stage parity.
/// Even stages → +2, odd stages → -2. Invisible networks → 0.
fn magic_delta(net_idx: usize) -> f64 {
    // Java: each network has an NStage. Stages are numbered 0, 1, 2, 3, ...
    // where network[0] gets stage 1 (createNewStage after initial null),
    // network[1] gets stage 3, etc. So network N has stage 2*N + 1.
    // isEven checks nstage.getNumber() % 2 == 0.
    // Stage 1 is odd → magicDelta = -2.
    // Stage 3 is odd → magicDelta = -2.
    // Actually, for simple cases all networks get odd stages, so magicDelta = -2.
    //
    // But wait, let's trace: NPlayField starts with empty stages list.
    // createNetwork("") calls: new Network(playField.getLast(), playField.createNewStage(), name)
    // First call: getLast() = null (stages empty), createNewStage() = getStage(0) → stage S0.
    // So network[0].nstage = S0 (number 0, even) → magicDelta = +2.
    // Second call: getLast() = S0, createNewStage() = getStage(1) → stage S1.
    // network[1].nstage = S1 (number 1, odd) → magicDelta = -2.
    //
    // Actually getLast returns stages[stages.size()-1] which after first createNewStage is S0.
    // For second network: getLast() = S0, createNewStage() = getStage(1) → S1.
    // network[1].up = S0, network[1].nstage = S1.
    // But wait, when a server connects to a second network, bar.addStage(network.getUp())
    // which is the stage BEFORE the network. So for network[1], up=S0.
    // The stages used in tetris are thus:
    // - web01 connects to dmz (stage S0), then to lan: bar.addStage(lan.up=S0) → no change.
    // Wait that doesn't seem right. Let me re-read.
    //
    // Actually: NPlayField.createNewStage creates stages sequentially: S0, S1, S2, ...
    // Network constructor: Network(up, nstage, name).
    // First network: up=null (getLast() when empty), nstage=S0. After: stages=[S0].
    // Second network: up=S0 (getLast()), createNewStage()=S1. After: stages=[S0, S1].
    // So network[0].nstage=S0 (number 0, even), network[1].nstage=S1 (number 1, odd).
    //
    // magicDelta: S0 is even → +2, S1 is odd → -2.

    let stage_number = net_idx; // For simple cases, network[i] gets stage i.
    if stage_number % 2 == 0 {
        2.0
    } else {
        -2.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::nwdiag::{Network, NwdiagDiagram, ServerRef};

    fn basic_diagram() -> NwdiagDiagram {
        NwdiagDiagram {
            title: Some("Infrastructure".to_string()),
            networks: vec![
                Network {
                    name: "dmz".to_string(),
                    address: Some("10.0.0.0/24".to_string()),
                    color: None,
                    servers: vec![
                        ServerRef {
                            name: "web01".to_string(),
                            address: Some("10.0.0.10".to_string()),
                            description: Some("frontend".to_string()),
                        },
                        ServerRef {
                            name: "db01".to_string(),
                            address: None,
                            description: None,
                        },
                    ],
                },
                Network {
                    name: "lan".to_string(),
                    address: None,
                    color: None,
                    servers: vec![
                        ServerRef {
                            name: "web01".to_string(),
                            address: None,
                            description: Some("app".to_string()),
                        },
                        ServerRef {
                            name: "app01".to_string(),
                            address: None,
                            description: None,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn resolve_description_last_wins() {
        let d = basic_diagram();
        // web01: described as "frontend" in dmz, then "app" in lan → "app" wins.
        assert_eq!(resolve_description("web01", &d), "app");
        // db01: no description → default name.
        assert_eq!(resolve_description("db01", &d), "db01");
        // app01: no description → default name.
        assert_eq!(resolve_description("app01", &d), "app01");
    }

    #[test]
    fn tetris_places_web01_col0_db01_col1_app01_col0() {
        let d = basic_diagram();
        let (order, conns) = build_connections(&d);
        let cols = tetris_layout(&order, &conns, d.networks.len());
        assert_eq!(cols["web01"], 0);
        assert_eq!(cols["db01"], 1);
        assert_eq!(cols["app01"], 0);
    }

    #[test]
    fn layout_produces_3_server_boxes() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        assert_eq!(layout.server_boxes.len(), 3);
        // Labels should use resolved descriptions.
        let labels: Vec<&str> = layout
            .server_boxes
            .iter()
            .map(|b| b.label.as_str())
            .collect();
        assert!(labels.contains(&"app"));
        assert!(labels.contains(&"db01"));
        assert!(labels.contains(&"app01"));
    }

    #[test]
    fn layout_produces_2_tubes() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        assert_eq!(layout.tubes.len(), 2);
    }

    #[test]
    fn layout_dmz_tube_wider_than_lan() {
        let d = basic_diagram();
        let layout = layout_nwdiag(&d).unwrap();
        // dmz spans cols 0 and 1, lan spans only col 0.
        assert!(layout.tubes[0].width > layout.tubes[1].width);
    }
}
