//! gitGraph data model. Mirrors the Commit/Branch shape from upstream
//! `gitGraphTypes.ts`. The model stores the parsed-and-resolved AST:
//! commits with branch + parent assignment already resolved (the
//! upstream `gitGraphAst.ts` walks the AST while mutating the
//! current-branch / head pointers; we do the same in `parser/gitgraph.rs`).

use crate::model::DiagramMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitKind {
    Normal,
    Reverse,
    Highlight,
    Merge,
    CherryPick,
}

impl CommitKind {
    pub fn class(&self) -> &'static str {
        match self {
            CommitKind::Normal => "commit-normal",
            CommitKind::Reverse => "commit-reverse",
            CommitKind::Highlight => "commit-highlight",
            CommitKind::Merge => "commit-merge",
            CommitKind::CherryPick => "commit-cherry-pick",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Commit {
    pub id: String,
    pub seq: usize,
    pub kind: CommitKind,
    /// Optional override-type when `commit type: REVERSE/HIGHLIGHT` is used.
    /// Mirrors upstream `customType` — distinct from `kind` only for merge
    /// commits where the user supplied a `type:` override.
    pub custom_type: Option<CommitKind>,
    pub custom_id: bool,
    pub tags: Vec<String>,
    pub parents: Vec<String>,
    pub branch: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub order: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    LR,
    TB,
    BT,
}

#[derive(Debug, Clone, Default)]
pub struct GitGraphConfig {
    pub rotate_commit_label: bool,
    pub show_branches: bool,
    pub show_commit_label: bool,
    pub parallel_commits: bool,
}

impl GitGraphConfig {
    pub fn defaults() -> Self {
        Self {
            rotate_commit_label: true,
            show_branches: true,
            show_commit_label: true,
            parallel_commits: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitGraphDiagram {
    pub meta: DiagramMeta,
    pub orientation: Orientation,
    pub config: GitGraphConfig,
    /// Branches in user-declared order (insertion order). Renderer
    /// derives display order separately if `branch ... order:` is used.
    pub branches: Vec<Branch>,
    /// Commits in chronological (seq) order.
    pub commits: Vec<Commit>,
    /// Optional theme override from frontmatter / init directive.
    pub theme_name: Option<String>,
    /// Whether the source contained a `%%{init:...}%%` directive — affects
    /// some preprocessing behavior parity with upstream.
    pub has_init_directive: bool,
}
