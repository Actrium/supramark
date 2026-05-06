// svek::builder - High-level diagram building via Graphviz
// Port of Java PlantUML's svek.GeneralImageBuilder, GraphvizImageBuilder,
// CucaDiagramFileMakerSvek
//
// Orchestrates: collect entities -> measure dimensions -> build DOT
//   -> run Graphviz -> parse SVG -> position entities

use crate::klimt::geom::{Rankdir, XDimension2D};
use crate::svek::cluster::Cluster;
use crate::svek::edge::{LabelDimension, SvekEdge};
use crate::svek::node::{EntityPosition, SvekNode};
use crate::svek::shape_type::ShapeType;
use crate::svek::SolveResult;
use crate::svek::{
    Bibliotekon, ColorSequence, DotMode, DotSplines, DotStringFactory, Margins, TopLevelDotItem,
};

use log::{debug, trace, warn};

// ── EntityDescriptor ────────────────────────────────────────────────

/// Describes an entity for the builder to create nodes/edges.
/// Lightweight data carrier -- avoids needing full Entity objects from the model layer.
#[derive(Debug, Clone)]
pub struct EntityDescriptor {
    pub id: String,
    pub width: f64,
    pub height: f64,
    pub shape_type: ShapeType,
    pub cluster_id: Option<String>,
    pub shield: Margins,
    pub entity_position: EntityPosition,
    pub max_label_width: f64,
    pub port_label_width: f64,
    pub order: Option<usize>,
    /// Whether this entity has been removed/hidden
    pub removed: bool,
    /// Extra LimitFinder left extension from entity image content.
    pub lf_extra_left: f64,
    /// Whether LimitFinder.drawRectangle -1 correction applies.
    /// False for notes (drawn with UPath instead of URectangle).
    pub lf_rect_correction: bool,
    /// Whether the entity draws a full-width body separator ULine.
    /// This overrides the drawRectangle -1 on max_x because the
    /// ULine contributes (x + width) without -1.
    pub lf_has_body_separator: bool,
    pub lf_node_polygon: bool,
    pub lf_polygon_hack: bool,
    /// Actor stickman: LimitFinder uses min_corr_y = -0.5, max_corr = (0, 0).
    pub lf_actor_stickman: bool,
    /// When true, the node appears in the DOT but is excluded from LimitFinder.
    /// Used for internal proxy/special-point nodes (Java: zaent).
    pub hidden: bool,
}

impl EntityDescriptor {
    pub fn new(id: &str, width: f64, height: f64) -> Self {
        Self {
            id: id.to_string(),
            width,
            height,
            shape_type: ShapeType::Rectangle,
            cluster_id: None,
            shield: Margins::none(),
            entity_position: EntityPosition::Normal,
            max_label_width: 0.0,
            port_label_width: 0.0,
            order: None,
            removed: false,
            lf_extra_left: 0.0,
            lf_rect_correction: true,
            lf_has_body_separator: false,
            lf_node_polygon: false,
            lf_polygon_hack: false,
            lf_actor_stickman: false,
            hidden: false,
        }
    }

    pub fn with_shape(mut self, shape: ShapeType) -> Self {
        self.shape_type = shape;
        self
    }

    pub fn with_cluster(mut self, cluster_id: &str) -> Self {
        self.cluster_id = Some(cluster_id.to_string());
        self
    }

    pub fn with_shield(mut self, shield: Margins) -> Self {
        self.shield = shield;
        self
    }

    pub fn with_entity_position(mut self, entity_position: EntityPosition) -> Self {
        self.entity_position = entity_position;
        self
    }

    pub fn with_max_label_width(mut self, max_label_width: f64) -> Self {
        self.max_label_width = max_label_width;
        self
    }

    pub fn with_port_label_width(mut self, port_label_width: f64) -> Self {
        self.port_label_width = port_label_width;
        self
    }

    pub fn with_order(mut self, order: usize) -> Self {
        self.order = Some(order);
        self
    }

    pub fn with_lf_extra_left(mut self, extra: f64) -> Self {
        self.lf_extra_left = extra;
        self
    }

    pub fn with_lf_rect_correction(mut self, val: bool) -> Self {
        self.lf_rect_correction = val;
        self
    }

    pub fn dimension(&self) -> XDimension2D {
        XDimension2D::new(self.width, self.height)
    }
}

// ── LinkDescriptor ──────────────────────────────────────────────────

/// Describes a link/edge between two entities.
#[derive(Debug, Clone)]
pub struct LinkDescriptor {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    /// Label dimensions (width, height) for DOT table sizing.
    /// Java: computed from TextBlock.calculateDimension().
    pub label_dimension: Option<(f64, f64)>,
    pub tail_label: Option<String>,
    pub tail_label_dimension: Option<(f64, f64)>,
    pub head_label: Option<String>,
    pub head_label_dimension: Option<(f64, f64)>,
    pub decor1: crate::svek::edge::LinkDecoration,
    pub decor2: crate::svek::edge::LinkDecoration,
    pub line_style: crate::svek::edge::LinkStyle,
    pub from_port: Option<String>,
    pub to_port: Option<String>,
    /// Whether this link has been removed
    pub removed: bool,
    /// Whether this link is invisible (used for layout constraint only)
    pub invisible: bool,
    /// Whether this link is rendered as an Opale connector ear instead of a
    /// regular edge.  Java: `SvekEdge.setOpale(true)`.  When set, the edge is
    /// laid out by Graphviz (so the spline endpoints are available) but is
    /// not drawn — an ear polygon is drawn by the note instead.
    pub is_opale: bool,
    /// Minimum edge length in DOT (minlen attribute). Default: use SvekEdge default.
    pub minlen: Option<u32>,
    /// Whether the link has a middle decoration (circle, diamond, etc.).
    /// Java: labelShield = 7 when middleDecor != NONE.
    pub has_middle_decor: bool,
    /// When true, set constraint=false in DOT (cross-axis direction hints).
    pub no_constraint: bool,
}

impl LinkDescriptor {
    pub fn new(from: &str, to: &str) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            label: None,
            label_dimension: None,
            tail_label: None,
            tail_label_dimension: None,
            head_label: None,
            head_label_dimension: None,
            decor1: crate::svek::edge::LinkDecoration::None,
            decor2: crate::svek::edge::LinkDecoration::None,
            line_style: crate::svek::edge::LinkStyle::Normal,
            from_port: None,
            to_port: None,
            removed: false,
            invisible: false,
            is_opale: false,
            minlen: None,
            has_middle_decor: false,
            no_constraint: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    pub fn with_tail_label(mut self, label: &str, width: f64, height: f64) -> Self {
        self.tail_label = Some(label.to_string());
        self.tail_label_dimension = Some((width, height));
        self
    }

    pub fn with_head_label(mut self, label: &str, width: f64, height: f64) -> Self {
        self.head_label = Some(label.to_string());
        self.head_label_dimension = Some((width, height));
        self
    }

    pub fn with_decorations(
        mut self,
        decor1: crate::svek::edge::LinkDecoration,
        decor2: crate::svek::edge::LinkDecoration,
    ) -> Self {
        self.decor1 = decor1;
        self.decor2 = decor2;
        self
    }

    pub fn with_style(mut self, line_style: crate::svek::edge::LinkStyle) -> Self {
        self.line_style = line_style;
        self
    }

    pub fn with_ports(mut self, from_port: Option<&str>, to_port: Option<&str>) -> Self {
        self.from_port = from_port.map(str::to_string);
        self.to_port = to_port.map(str::to_string);
        self
    }
}

// ── ClusterDescriptor ───────────────────────────────────────────────

/// Describes a cluster (package/group) containing entities.
#[derive(Debug, Clone)]
pub struct ClusterDescriptor {
    pub id: String,
    pub title: Option<String>,
    pub style: super::cluster::ClusterStyle,
    pub label_size: Option<(f64, f64)>,
    pub entity_ids: Vec<String>,
    pub sub_clusters: Vec<ClusterDescriptor>,
    pub order: Option<usize>,
    /// Java: thereALinkFromOrToGroup — generates `_a` / `_i` wrapper subgraphs.
    pub has_link_from_or_to_group: bool,
    /// DOT node id of the special point inside the cluster (Java: zaent).
    pub special_point_id: Option<String>,
}

impl ClusterDescriptor {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            title: None,
            style: super::cluster::ClusterStyle::default(),
            label_size: None,
            entity_ids: Vec::new(),
            sub_clusters: Vec::new(),
            order: None,
            has_link_from_or_to_group: false,
            special_point_id: None,
        }
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    pub fn with_label_size(mut self, width: f64, height: f64) -> Self {
        self.label_size = Some((width, height));
        self
    }

    pub fn with_style(mut self, style: super::cluster::ClusterStyle) -> Self {
        self.style = style;
        self
    }

    pub fn add_entity(mut self, entity_id: &str) -> Self {
        self.entity_ids.push(entity_id.to_string());
        self
    }

    pub fn add_sub_cluster(mut self, sub: ClusterDescriptor) -> Self {
        self.sub_clusters.push(sub);
        self
    }

    pub fn with_order(mut self, order: usize) -> Self {
        self.order = Some(order);
        self
    }
}

// ── BuilderConfig ───────────────────────────────────────────────────

/// Configuration for the Graphviz image builder.
#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub rankdir: Rankdir,
    pub dot_splines: DotSplines,
    pub dot_mode: DotMode,
    pub is_activity: bool,
    pub is_state: bool,
    pub nodesep: Option<f64>,
    pub ranksep: Option<f64>,
    pub use_simplier_dot_link_strategy: bool,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            rankdir: Rankdir::TopToBottom,
            dot_splines: DotSplines::Spline,
            dot_mode: DotMode::Normal,
            is_activity: false,
            is_state: false,
            nodesep: None,
            ranksep: None,
            use_simplier_dot_link_strategy: false,
        }
    }
}

// ── GraphvizImageBuilder ────────────────────────────────────────────

/// High-level builder that orchestrates the Graphviz layout workflow.
/// Java: `svek.GraphvizImageBuilder` + `svek.CucaDiagramFileMakerSvek`
///
/// Workflow:
/// 1. `add_entities()` / `add_links()` / `add_clusters()` -- register elements
/// 2. `build_dot()` -- generate DOT string
/// 3. `solve()` -- parse SVG output and position elements
/// 4. Access positioned nodes/edges via the bibliotekon
pub struct GraphvizImageBuilder {
    config: BuilderConfig,
    entities: Vec<EntityDescriptor>,
    links: Vec<LinkDescriptor>,
    clusters: Vec<ClusterDescriptor>,
    color_seq: ColorSequence,
    factory: Option<DotStringFactory>,
}

impl GraphvizImageBuilder {
    pub fn new(config: BuilderConfig) -> Self {
        Self {
            config,
            entities: Vec::new(),
            links: Vec::new(),
            clusters: Vec::new(),
            color_seq: ColorSequence::new(),
            factory: None,
        }
    }

    /// Add a single entity descriptor.
    pub fn add_entity(&mut self, entity: EntityDescriptor) {
        self.entities.push(entity);
    }

    /// Add multiple entity descriptors.
    pub fn add_entities(&mut self, entities: Vec<EntityDescriptor>) {
        self.entities.extend(entities);
    }

    /// Add a single link descriptor.
    pub fn add_link(&mut self, link: LinkDescriptor) {
        self.links.push(link);
    }

    /// Add multiple link descriptors.
    pub fn add_links(&mut self, links: Vec<LinkDescriptor>) {
        self.links.extend(links);
    }

    /// Add a cluster descriptor.
    pub fn add_cluster(&mut self, cluster: ClusterDescriptor) {
        self.clusters.push(cluster);
    }

    /// Check if the diagram is degenerate (zero or one entity).
    /// Java: `DotData.isDegeneratedWithFewEntities()`
    pub fn is_degenerate(&self) -> bool {
        let active = self.entities.iter().filter(|e| !e.removed).count();
        active <= 1
    }

    /// Build the complete DOT string from all registered elements.
    /// Java: `GraphvizImageBuilder.buildImage()` (the DOT generation part)
    ///
    /// Returns the DOT string. Also populates the internal DotStringFactory
    /// for later use by `solve()`.
    pub fn build_dot(&mut self) -> String {
        debug!(
            "Building DOT string for {} entities, {} links, {} clusters",
            self.entities.len(),
            self.links.len(),
            self.clusters.len()
        );

        let mut bib = Bibliotekon::new();

        // Create nodes from entity descriptors
        for ent in &self.entities {
            if ent.removed {
                trace!("Skipping removed entity: {}", ent.id);
                continue;
            }
            let mut node = SvekNode::new(&ent.id, ent.width, ent.height);
            node.color = self.color_seq.next_color();
            node.shape_type = ent.shape_type;
            node.cluster_id = ent.cluster_id.clone();
            node.shield = ent.shield;
            node.entity_position = ent.entity_position;
            node.max_label_width = ent.max_label_width;
            node.port_label_width = ent.port_label_width;
            node.lf_extra_left = ent.lf_extra_left;
            node.lf_rect_correction = ent.lf_rect_correction;
            node.lf_has_body_separator = ent.lf_has_body_separator;
            node.lf_node_polygon = ent.lf_node_polygon;
            node.lf_polygon_hack = ent.lf_polygon_hack;
            node.lf_actor_stickman = ent.lf_actor_stickman;
            node.hidden = ent.hidden;
            bib.add_node(node);
        }

        // Create edges from link descriptors
        for link in &self.links {
            if link.removed {
                trace!("Skipping removed link: {} -> {}", link.from, link.to);
                continue;
            }
            // Verify both endpoints exist
            if bib.find_node(&link.from).is_none() {
                warn!("Link source '{}' not found in nodes, skipping", link.from);
                continue;
            }
            if bib.find_node(&link.to).is_none() {
                warn!("Link target '{}' not found in nodes, skipping", link.to);
                continue;
            }
            let mut edge = SvekEdge::new(&link.from, &link.to);
            edge.color = self.color_seq.next_color();
            edge.note_label_color = self.color_seq.next_color();
            edge.start_tail_color = self.color_seq.next_color();
            edge.end_head_color = self.color_seq.next_color();
            if link.from_port.is_some() || link.to_port.is_some() {
                let start = match &link.from_port {
                    Some(port) => crate::svek::edge::EntityPort::with_port(&link.from, port),
                    None => crate::svek::edge::EntityPort::new(&link.from),
                };
                let end = match &link.to_port {
                    Some(port) => crate::svek::edge::EntityPort::with_port(&link.to, port),
                    None => crate::svek::edge::EntityPort::new(&link.to),
                };
                edge = edge.with_ports(start, end);
            }
            edge = edge.with_decorations(link.decor1, link.decor2);
            edge = edge.with_style(link.line_style);
            edge.use_simplier_dot_link_strategy = self.config.use_simplier_dot_link_strategy;
            edge.label = link.label.clone();
            if let Some((w, h)) = link.label_dimension {
                edge.label_dimension = Some(LabelDimension::new(w, h));
                // Java: SvekLine.labelShield = 7 only when middle decoration is present
                // (e.g., circle/diamond on link). For plain association links, shield = 0.
                // middle_decor support: edge.label_shield = 7.0 when link has circle/diamond.
                edge.label_shield = if link.has_middle_decor { 7.0 } else { 0.0 };
            }
            if let (Some(text), Some((w, h))) = (&link.tail_label, link.tail_label_dimension) {
                edge = edge.with_tail_label(text, LabelDimension::new(w, h));
            }
            if let (Some(text), Some((w, h))) = (&link.head_label, link.head_label_dimension) {
                edge = edge.with_head_label(text, LabelDimension::new(w, h));
            }
            edge.is_invis = link.invisible;
            edge.opale = link.is_opale;
            if link.no_constraint {
                edge.is_constraint = false;
            }
            if let Some(minlen) = link.minlen {
                // link_length = minlen + 1 (Java: minlen = link.getLength() - 1)
                edge.link_length = (minlen as i32) + 1;
            }
            bib.add_edge(edge);
        }

        // Create clusters from cluster descriptors
        for cdesc in &self.clusters {
            let cluster = Self::build_cluster(cdesc);
            bib.add_cluster(cluster);
        }

        // nodes-before-clusters: see Java printCluster2()
        let mut top_level_items: Vec<(usize, usize, usize, TopLevelDotItem)> = Vec::new();
        for (idx, ent) in self.entities.iter().enumerate() {
            if ent.removed || ent.cluster_id.is_some() {
                continue;
            }
            if let Some(order) = ent.order {
                top_level_items.push((0, order, idx, TopLevelDotItem::Node(ent.id.clone())));
            }
        }
        let cluster_base = self.entities.len();
        for (idx, cluster) in self.clusters.iter().enumerate() {
            if let Some(order) = cluster.order {
                top_level_items.push((
                    1,
                    order,
                    cluster_base + idx,
                    TopLevelDotItem::Cluster(cluster.id.clone()),
                ));
            }
        }
        top_level_items.sort_by_key(|(type_prio, order, idx, _)| (*type_prio, *order, *idx));

        // Build the factory and generate DOT
        let mut factory = DotStringFactory::new(bib)
            .with_rankdir(self.config.rankdir)
            .with_splines(self.config.dot_splines)
            .with_activity(self.config.is_activity)
            .with_top_level_items(
                top_level_items
                    .into_iter()
                    .map(|(_, _, _, item)| item)
                    .collect(),
            );

        if let Some(nodesep) = self.config.nodesep {
            factory.nodesep_override = Some(nodesep);
        }
        if let Some(ranksep) = self.config.ranksep {
            factory.ranksep_override = Some(ranksep);
        }

        let dot = factory.create_dot_string(self.config.dot_mode);
        self.factory = Some(factory);
        dot
    }

    /// Recursively build a Cluster from a ClusterDescriptor.
    fn build_cluster(cdesc: &ClusterDescriptor) -> Cluster {
        let mut cluster = Cluster::new(&cdesc.id);
        cluster.title = cdesc.title.clone();
        cluster.style = cdesc.style;
        cluster.label_size = cdesc.label_size;
        cluster.has_link_from_or_to_group = cdesc.has_link_from_or_to_group;
        cluster.special_point_id = cdesc.special_point_id.clone();
        for eid in &cdesc.entity_ids {
            cluster.add_node(eid);
        }
        for sub in &cdesc.sub_clusters {
            cluster.sub_clusters.push(Self::build_cluster(sub));
        }
        cluster
    }

    /// Parse Graphviz SVG output and position all elements.
    /// Java: `DotStringFactory.solve()` + `GraphvizImageBuilder.buildImage()` (solve part)
    ///
    /// Call this after running Graphviz externally and obtaining the SVG output.
    /// Returns (moveDelta, limitFinder_span, lf_max, render_offset) from normalization.
    pub fn solve(&mut self, svg: &str) -> SolveResult {
        let factory = self
            .factory
            .as_mut()
            .ok_or_else(|| "Must call build_dot() before solve()".to_string())?;

        debug!("Solving SVG output ({} bytes)", svg.len());
        factory.solve(svg)
    }

    /// Access the positioned bibliotekon after solving.
    pub fn bibliotekon(&self) -> Option<&Bibliotekon> {
        self.factory.as_ref().map(|f| &f.bibliotekon)
    }

    /// Access the positioned bibliotekon mutably.
    pub fn bibliotekon_mut(&mut self) -> Option<&mut Bibliotekon> {
        self.factory.as_mut().map(|f| &mut f.bibliotekon)
    }

    /// Get the positioned nodes after solving.
    pub fn nodes(&self) -> &[SvekNode] {
        self.factory
            .as_ref()
            .map(|f| f.bibliotekon.nodes.as_slice())
            .unwrap_or(&[])
    }

    /// Get the positioned edges after solving.
    pub fn edges(&self) -> &[SvekEdge] {
        self.factory
            .as_ref()
            .map(|f| f.bibliotekon.edges.as_slice())
            .unwrap_or(&[])
    }

    /// Get the positioned clusters after solving.
    pub fn clusters(&self) -> &[Cluster] {
        self.factory
            .as_ref()
            .map(|f| f.bibliotekon.clusters.as_slice())
            .unwrap_or(&[])
    }

    /// Move all positioned elements by a delta offset.
    pub fn move_delta(&mut self, dx: f64, dy: f64) {
        if let Some(factory) = &mut self.factory {
            factory.move_delta(dx, dy);
        }
    }

    /// Get the DOT string factory (if built).
    pub fn factory(&self) -> Option<&DotStringFactory> {
        self.factory.as_ref()
    }
}

// ── GeneralImageBuilder ─────────────────────────────────────────────

/// Static helpers for entity image creation.
/// Java: `svek.GeneralImageBuilder`
///
/// Maps entity leaf types to their corresponding image implementations.
/// In the Rust port, this is simplified to shape type selection since
/// actual rendering is done by the SVG renderer.
pub struct GeneralImageBuilder;

impl GeneralImageBuilder {
    /// Determine the shape type for a given leaf type string.
    /// Java: `GeneralImageBuilder.createEntityImageBlock()`
    ///
    /// This is a simplified mapping -- full Java implementation dispatches
    /// to ~40 different EntityImage subclasses.
    pub fn shape_for_leaf_type(leaf_type: &str) -> ShapeType {
        match leaf_type {
            "class" | "interface" | "abstract" | "enum" | "annotation" => ShapeType::Rectangle,
            "note" => ShapeType::Rectangle,
            "activity" => ShapeType::RoundRectangle,
            "state" => ShapeType::RoundRectangle,
            "circle_start" => ShapeType::Circle,
            "circle_end" => ShapeType::Circle,
            "branch" | "state_choice" => ShapeType::Diamond,
            "usecase" | "description" => ShapeType::Oval,
            "object" | "map" | "json" => ShapeType::Rectangle,
            "synchro_bar" | "state_fork_join" => ShapeType::Rectangle,
            "hexagon" => ShapeType::Hexagon,
            "octagon" => ShapeType::Octagon,
            "folder" => ShapeType::Folder,
            "empty_package" => ShapeType::Rectangle,
            _ => ShapeType::Rectangle,
        }
    }

    /// Default forced stroke thickness for package borders.
    /// Java: `GeneralImageBuilder.getForcedStroke()`
    pub const DEFAULT_PACKAGE_BORDER_THICKNESS: f64 = 1.5;
}

// ── Convenience functions ───────────────────────────────────────────

/// Order links such that links with the same connections are grouped together.
/// Java: `CucaDiagramFileMakerSvek.getOrderedLinks()`
pub fn order_links(links: Vec<LinkDescriptor>) -> Vec<LinkDescriptor> {
    let mut result: Vec<LinkDescriptor> = Vec::new();
    for link in links {
        let insert_pos = find_link_insert_position(&result, &link);
        result.insert(insert_pos, link);
    }
    result
}

/// Find the position to insert a link to keep same-connection links grouped.
/// Java: `CucaDiagramFileMakerSvek.addLinkNew()`
fn find_link_insert_position(result: &[LinkDescriptor], link: &LinkDescriptor) -> usize {
    for i in 0..result.len() {
        if same_connections(&result[i], link) {
            // Find end of the group with same connections
            let mut j = i;
            while j < result.len() && same_connections(&result[j], link) {
                j += 1;
            }
            return j;
        }
    }
    result.len()
}

/// Check if two links connect the same pair of entities (in either direction).
fn same_connections(a: &LinkDescriptor, b: &LinkDescriptor) -> bool {
    (a.from == b.from && a.to == b.to) || (a.from == b.to && a.to == b.from)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_descriptor_basic() {
        let ed = EntityDescriptor::new("Foo", 100.0, 50.0)
            .with_shape(ShapeType::RoundRectangle)
            .with_cluster("pkg1");
        assert_eq!(ed.id, "Foo");
        assert_eq!(ed.width, 100.0);
        assert_eq!(ed.height, 50.0);
        assert_eq!(ed.shape_type, ShapeType::RoundRectangle);
        assert_eq!(ed.cluster_id.as_deref(), Some("pkg1"));
        assert!(!ed.removed);
    }

    #[test]
    fn link_descriptor_basic() {
        let ld = LinkDescriptor::new("A", "B").with_label("extends");
        assert_eq!(ld.from, "A");
        assert_eq!(ld.to, "B");
        assert_eq!(ld.label.as_deref(), Some("extends"));
    }

    #[test]
    fn cluster_descriptor_basic() {
        let cd = ClusterDescriptor::new("pkg")
            .with_title("MyPackage")
            .add_entity("A")
            .add_entity("B");
        assert_eq!(cd.id, "pkg");
        assert_eq!(cd.title.as_deref(), Some("MyPackage"));
        assert_eq!(cd.entity_ids.len(), 2);
    }

    #[test]
    fn builder_is_degenerate() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        assert!(builder.is_degenerate()); // 0 entities

        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        assert!(builder.is_degenerate()); // 1 entity

        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));
        assert!(!builder.is_degenerate()); // 2 entities
    }

    #[test]
    fn builder_build_dot() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));
        builder.add_link(LinkDescriptor::new("A", "B"));

        let dot = builder.build_dot();
        assert!(dot.contains("digraph unix"));
        assert!(dot.contains("\"A\" ["));
        assert!(dot.contains("\"B\" ["));
        assert!(dot.contains("\"A\"->\"B\""));
    }

    #[test]
    fn builder_build_dot_with_cluster() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0).with_cluster("pkg"));
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));
        builder.add_cluster(
            ClusterDescriptor::new("pkg")
                .with_title("MyPackage")
                .add_entity("A"),
        );

        let dot = builder.build_dot();
        assert!(dot.contains("subgraph cluster_pkg"));
        assert!(dot.contains("subgraph cluster_pkgp0"));
        assert!(dot.contains("subgraph cluster_pkgp1"));
        assert!(dot.contains("labeljust=\"c\""));
        assert!(dot.contains("style=solid;"));
        assert!(dot.contains("color=\"#000000\";"));
        assert!(dot.contains("label=<"));
    }

    #[test]
    fn builder_build_dot_with_shielded_rectangle() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(
            EntityDescriptor::new("A", 100.0, 50.0).with_shield(Margins::new(5.0, 10.0, 3.0, 7.0)),
        );

        let dot = builder.build_dot();
        assert!(dot.contains("shape=plaintext"));
        assert!(dot.contains("CELLSPACING=\"0\""));
        assert!(dot.contains("WIDTH=\"5.0\" HEIGHT=\"1.0\""));
        assert!(dot.contains("WIDTH=\"10.0\" HEIGHT=\"1.0\""));
        assert!(dot.contains("WIDTH=\"1.0\" HEIGHT=\"3.0\""));
        assert!(dot.contains("WIDTH=\"1.0\" HEIGHT=\"7.0\""));
        assert!(dot.contains("PORT=\"h\""));
    }

    #[test]
    fn builder_skips_removed_entities() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        let mut ent = EntityDescriptor::new("A", 100.0, 50.0);
        ent.removed = true;
        builder.add_entity(ent);
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));

        let dot = builder.build_dot();
        assert!(!dot.contains("\"A\" ["));
        assert!(dot.contains("\"B\" ["));
    }

    #[test]
    fn builder_skips_removed_links() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));
        let mut link = LinkDescriptor::new("A", "B");
        link.removed = true;
        builder.add_link(link);

        let dot = builder.build_dot();
        assert!(!dot.contains("\"A\"->\"B\""));
    }

    #[test]
    fn builder_skips_links_with_missing_endpoints() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        builder.add_link(LinkDescriptor::new("A", "MISSING"));

        let dot = builder.build_dot();
        // Link should be skipped since "MISSING" node doesn't exist
        assert!(!dot.contains("A->MISSING"));
    }

    #[test]
    fn builder_solve_requires_build() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        let result = builder.solve("<svg/>");
        assert!(result.is_err());
    }

    #[test]
    fn builder_nodes_before_build() {
        let builder = GraphvizImageBuilder::new(BuilderConfig::default());
        assert!(builder.nodes().is_empty());
    }

    #[test]
    fn builder_config_activity() {
        let config = BuilderConfig {
            is_activity: true,
            ..Default::default()
        };
        let mut builder = GraphvizImageBuilder::new(config);
        builder
            .add_entity(EntityDescriptor::new("start", 20.0, 20.0).with_shape(ShapeType::Circle));
        builder.add_entity(
            EntityDescriptor::new("act1", 100.0, 50.0).with_shape(ShapeType::RoundRectangle),
        );
        builder.add_link(LinkDescriptor::new("start", "act1"));

        let dot = builder.build_dot();
        assert!(dot.contains("digraph unix"));
        assert!(dot.contains("shape=circle"));
    }

    #[test]
    fn general_image_builder_shape_mapping() {
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("class"),
            ShapeType::Rectangle
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("activity"),
            ShapeType::RoundRectangle
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("state"),
            ShapeType::RoundRectangle
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("circle_start"),
            ShapeType::Circle
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("branch"),
            ShapeType::Diamond
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("usecase"),
            ShapeType::Oval
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("hexagon"),
            ShapeType::Hexagon
        );
        assert_eq!(
            GeneralImageBuilder::shape_for_leaf_type("unknown"),
            ShapeType::Rectangle
        );
    }

    #[test]
    fn order_links_basic() {
        let links = vec![
            LinkDescriptor::new("A", "B"),
            LinkDescriptor::new("C", "D"),
            LinkDescriptor::new("A", "B"),
        ];
        let ordered = order_links(links);
        assert_eq!(ordered.len(), 3);
        // Same-connection links should be grouped
        assert_eq!(ordered[0].from, "A");
        assert_eq!(ordered[1].from, "A");
        assert_eq!(ordered[2].from, "C");
    }

    #[test]
    fn order_links_reverse_same_connection() {
        let links = vec![
            LinkDescriptor::new("A", "B"),
            LinkDescriptor::new("C", "D"),
            LinkDescriptor::new("B", "A"), // reverse of A->B
        ];
        let ordered = order_links(links);
        assert_eq!(ordered.len(), 3);
        // A->B and B->A should be grouped together
        assert!(
            (ordered[0].from == "A" && ordered[1].from == "B")
                || (ordered[0].from == "B" && ordered[1].from == "A")
        );
    }

    #[test]
    fn same_connections_check() {
        let a = LinkDescriptor::new("X", "Y");
        let b = LinkDescriptor::new("X", "Y");
        let c = LinkDescriptor::new("Y", "X");
        let d = LinkDescriptor::new("X", "Z");
        assert!(same_connections(&a, &b));
        assert!(same_connections(&a, &c));
        assert!(!same_connections(&a, &d));
    }

    #[test]
    fn builder_rankdir_lr() {
        let config = BuilderConfig {
            rankdir: Rankdir::LeftToRight,
            ..Default::default()
        };
        let mut builder = GraphvizImageBuilder::new(config);
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        let dot = builder.build_dot();
        assert!(dot.contains("rankdir=LR"));
    }

    #[test]
    fn builder_splines_ortho() {
        let config = BuilderConfig {
            dot_splines: DotSplines::Ortho,
            ..Default::default()
        };
        let mut builder = GraphvizImageBuilder::new(config);
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        let dot = builder.build_dot();
        assert!(dot.contains("splines=ortho"));
    }

    #[test]
    fn builder_custom_nodesep_ranksep() {
        let config = BuilderConfig {
            nodesep: Some(50.0),
            ranksep: Some(80.0),
            ..Default::default()
        };
        let mut builder = GraphvizImageBuilder::new(config);
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        let dot = builder.build_dot();
        // 50px / 72 = 0.694444
        assert!(dot.contains("nodesep=0.694444"));
        // 80px / 72 = 1.111111
        assert!(dot.contains("ranksep=1.111111"));
    }

    #[test]
    fn builder_nested_cluster() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0).with_cluster("outer"));
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0).with_cluster("inner"));
        builder.add_cluster(
            ClusterDescriptor::new("outer")
                .with_title("Outer")
                .add_entity("A")
                .add_sub_cluster(
                    ClusterDescriptor::new("inner")
                        .with_title("Inner")
                        .add_entity("B"),
                ),
        );

        let dot = builder.build_dot();
        assert!(dot.contains("subgraph cluster_outer"));
        assert!(dot.contains("subgraph cluster_inner"));
        assert!(dot.contains("subgraph cluster_outerp0"));
        assert!(dot.contains("subgraph cluster_outerp1"));
        assert!(dot.contains("subgraph cluster_innerp0"));
        assert!(dot.contains("subgraph cluster_innerp1"));
        assert!(dot.contains("labeljust=\"c\""));
        assert!(dot.matches("label=<").count() >= 2);
    }

    #[test]
    fn builder_move_delta() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        builder.build_dot();

        // Before move
        let nodes = builder.nodes();
        assert_eq!(nodes.len(), 1);
        let orig_cx = nodes[0].cx;
        let orig_cy = nodes[0].cy;

        builder.move_delta(10.0, 20.0);
        let nodes = builder.nodes();
        assert_eq!(nodes[0].cx, orig_cx + 10.0);
        assert_eq!(nodes[0].cy, orig_cy + 20.0);
    }

    #[test]
    fn builder_edge_with_label() {
        let mut builder = GraphvizImageBuilder::new(BuilderConfig::default());
        builder.add_entity(EntityDescriptor::new("A", 100.0, 50.0));
        builder.add_entity(EntityDescriptor::new("B", 80.0, 40.0));
        builder.add_link(LinkDescriptor::new("A", "B").with_label("inherits"));

        let dot = builder.build_dot();
        // svek edge label is rendered as HTML table, not plain text
        assert!(dot.contains("label=<"));
    }

    #[test]
    fn entity_descriptor_dimension() {
        let ed = EntityDescriptor::new("Foo", 120.0, 60.0);
        let dim = ed.dimension();
        assert_eq!(dim.width, 120.0);
        assert_eq!(dim.height, 60.0);
    }
}
