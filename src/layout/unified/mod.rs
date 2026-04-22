//! Unified rendering pipeline — Wave 3 P0 foundation.
//!
//! Ports the core of upstream's `packages/mermaid/src/rendering-util/`
//! directory: the shared `LayoutData` / `Node` / `Edge` shape and the
//! layout-engine dispatcher.
//!
//! Scope of the port is **foundation only** — Stratum 3 diagrams
//! (er / requirement / class / state / flowchart / block) plug into this
//! module in Wave 4.
//!
//! Upstream references:
//! * `rendering-util/types.ts`  (209 LoC) — `types.rs`
//! * `rendering-util/render.ts` (146 LoC) — `render.rs`

pub mod render;
pub mod types;

pub use render::{layout, registered_algorithms, DEFAULT_ALGORITHM};
pub use types::{
    AssetPos, Bounds, Cluster, Constraint, Edge, LayoutData, LayoutMethod, LayoutResult,
    MarkdownLine, MarkdownWord, MarkdownWordType, MindmapOptions, Node, NodeChildren, Point,
    RectOptions, RenderData, ShapeRenderOptions,
};
