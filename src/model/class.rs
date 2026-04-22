//! Class diagram AST — mirrors upstream `classTypes.ts` and the tree
//! `classDb.ts` builds from parser callbacks.
//!
//! Upstream references:
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classTypes.ts`
//! * `/ext/mermaid-official-stable-v11.14.0/packages/mermaid/src/diagrams/class/classDb.ts`
//!
//! The parser populates a [`ClassDiagram`] and hands it to the layout
//! stage; the layout stage converts it into `unified::LayoutData` for
//! dagre, then the renderer walks both to produce SVG.
//!
//! Fidelity notes
//! --------------
//! * We keep *per-member* classification fields (`visibility`,
//!   `classifier`, `parameters`, `return_type`) rather than a pre-joined
//!   text blob, because the renderer needs them for style decisions
//!   (`*` → italic abstract, `$` → underlined static). See
//!   `ClassMember::get_display_details` upstream.
//! * Namespaces (`namespace Foo { class Bar {} }`) are represented as a
//!   flat list with each class carrying an optional `parent` pointer —
//!   identical to upstream's `NamespaceNode::classes` map plus the
//!   `class.parent` backref.
//! * The grammar also accepts both `classDiagram` (v1) and
//!   `classDiagram-v2` tokens; the v3 unified renderer treats them
//!   identically (cf. `classRenderer-v3-unified.ts`).

use crate::model::DiagramMeta;

/// A parsed class diagram. One of these becomes a `Diagram::Class` via
/// the (currently unit-variant) placeholder in `model/mod.rs`.
#[derive(Debug, Clone, Default)]
pub struct ClassDiagram {
    pub meta: DiagramMeta,
    /// `"TB" | "BT" | "LR" | "RL"` — top-level direction. Upstream
    /// default is `"TB"`.
    pub direction: Option<String>,
    /// Classes in declaration order; the parser also guarantees any
    /// class referenced implicitly (e.g. as a relation endpoint) is
    /// appended with defaults.
    pub classes: Vec<ClassNode>,
    /// Namespaces — cluster boxes that wrap a subset of classes.
    pub namespaces: Vec<Namespace>,
    /// Relations between classes (inheritance, composition, etc.).
    pub relations: Vec<ClassRelation>,
    /// Stand-alone notes (`note "..."`) and per-class notes
    /// (`note for Foo "..."`).
    pub notes: Vec<ClassNote>,
    /// `classDef foo fill:#f9f` — reusable style class definitions.
    pub style_classes: Vec<StyleClass>,
    /// `click Foo href "..."` interactivity.
    pub interactivity: Vec<ClassInteractivity>,
    /// True if the diagram was introduced with `classDiagram-v2`
    /// rather than `classDiagram`. Rendering is unchanged; kept for
    /// completeness.
    pub v2: bool,
}

impl ClassDiagram {
    /// Find (mutable) a class by its id, or create it if missing.
    /// Upstream's `addClass` idempotently appends.
    pub fn class_mut(&mut self, id: &str) -> &mut ClassNode {
        if let Some(idx) = self.classes.iter().position(|c| c.id == id) {
            return &mut self.classes[idx];
        }
        self.classes.push(ClassNode::new(id));
        self.classes.last_mut().unwrap()
    }

    /// Look a class up by id (read-only).
    pub fn class(&self, id: &str) -> Option<&ClassNode> {
        self.classes.iter().find(|c| c.id == id)
    }
}

/// Visibility marker for a member. Upstream union `'#' | '+' | '~' | '-' | ''`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    None,
    /// `+` — public
    Public,
    /// `-` — private
    Private,
    /// `#` — protected
    Protected,
    /// `~` — package / internal
    Package,
}

impl Visibility {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '+' => Some(Self::Public),
            '-' => Some(Self::Private),
            '#' => Some(Self::Protected),
            '~' => Some(Self::Package),
            _ => None,
        }
    }

    /// Returns the glyph mermaid renders inside the class box.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Public => "+",
            Self::Private => "-",
            Self::Protected => "#",
            Self::Package => "~",
        }
    }
}

/// Trailing classifier on a member — `*` abstract / `$` static.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Classifier {
    #[default]
    None,
    /// `*` — abstract (rendered italic)
    Abstract,
    /// `$` — static (rendered underlined)
    Static,
}

impl Classifier {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            '*' => Some(Self::Abstract),
            '$' => Some(Self::Static),
            _ => None,
        }
    }

    /// CSS fragment upstream's `parseClassifier` produces.
    pub fn css(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Abstract => "font-style:italic;",
            Self::Static => "text-decoration:underline;",
        }
    }
}

/// `'method' | 'attribute'` — what kind of slot a [`ClassMember`] fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Attribute,
    Method,
}

/// One attribute or method inside a class body. Fields mirror upstream
/// `ClassMember`.
#[derive(Debug, Clone)]
pub struct ClassMember {
    pub kind: MemberKind,
    /// Raw identifier — the name minus visibility prefix and classifier
    /// suffix, but *including* any type annotation for attributes.
    pub id: String,
    pub visibility: Visibility,
    pub classifier: Classifier,
    /// For methods only — `""` when no params or not a method.
    pub parameters: String,
    /// For methods only — `""` when not present.
    pub return_type: String,
    /// Pre-formatted display text (visibility + generics + params +
    /// return) used by the renderer. Mirrors upstream `text` field.
    pub text: String,
    /// CSS style from the classifier (`font-style:italic;` etc.).
    pub css_style: String,
}

impl ClassMember {
    pub fn new(kind: MemberKind) -> Self {
        Self {
            kind,
            id: String::new(),
            visibility: Visibility::None,
            classifier: Classifier::None,
            parameters: String::new(),
            return_type: String::new(),
            text: String::new(),
            css_style: String::new(),
        }
    }
}

/// Parsed class declaration. Mirrors `ClassNode` upstream.
#[derive(Debug, Clone)]
pub struct ClassNode {
    pub id: String,
    /// Human-readable label. For most classes this equals `id`; set
    /// explicitly when the declaration used `class Foo["..."]`.
    pub label: String,
    /// Base id (pre-generic) — `"Foo"` for `Foo~T~`.
    pub base_id: String,
    /// Generic type argument if declared `Foo~T~`. `None` otherwise.
    pub generic: Option<String>,
    /// `<<interface>>`, `<<service>>`, etc.
    pub annotations: Vec<String>,
    pub members: Vec<ClassMember>,
    pub methods: Vec<ClassMember>,
    /// `cssClass Foo bar` — applies `.bar` to the rendered group.
    pub css_classes: Vec<String>,
    /// `style Foo fill:#f9f` — inline style string.
    pub styles: Vec<String>,
    /// DOM id used by upstream `insertElementsForSize`. We generate ours
    /// to match: `classId-<id>-<counter>`.
    pub dom_id: String,
    /// Enclosing namespace id, if any.
    pub parent: Option<String>,
    /// `link Foo "https://..."` — optional hyperlink.
    pub link: Option<String>,
    pub link_target: Option<String>,
    /// `click Foo callback()` flag.
    pub have_callback: bool,
    /// Tooltip attached via `click Foo cb "tip"`.
    pub tooltip: Option<String>,
}

impl ClassNode {
    pub fn new(id: &str) -> Self {
        let (base, generic) = split_generic(id);
        Self {
            id: id.to_string(),
            label: base.to_string(),
            base_id: base.to_string(),
            generic: generic.map(str::to_string),
            annotations: Vec::new(),
            members: Vec::new(),
            methods: Vec::new(),
            css_classes: Vec::new(),
            styles: Vec::new(),
            dom_id: String::new(),
            parent: None,
            link: None,
            link_target: None,
            have_callback: false,
            tooltip: None,
        }
    }
}

/// Split `Foo~T~` into (`"Foo"`, `Some("T")`). For `Foo` returns
/// (`"Foo"`, `None`). Generic tails may contain nested `~`, but the
/// outer split here is consistent with upstream's `className` grammar
/// rule.
fn split_generic(name: &str) -> (&str, Option<&str>) {
    if let Some((head, tail)) = name.split_once('~') {
        let tail = tail.strip_suffix('~').unwrap_or(tail);
        (head, Some(tail))
    } else {
        (name, None)
    }
}

/// `namespace Foo { class Bar {} }` — flat list + classes reference via
/// `ClassNode::parent`.
#[derive(Debug, Clone)]
pub struct Namespace {
    pub id: String,
    pub dom_id: String,
    /// Ids of direct children classes (order of declaration).
    pub class_ids: Vec<String>,
    /// Ids of direct children notes.
    pub note_ids: Vec<String>,
}

/// Relation type from upstream `relationType` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEnd {
    /// None — plain line end.
    None,
    /// `o` — hollow diamond (aggregation)
    Aggregation,
    /// `<|` / `|>` — hollow triangle (inheritance)
    Extension,
    /// `*` — filled diamond (composition)
    Composition,
    /// `>` / `<` — open arrow (dependency)
    Dependency,
    /// `()` — lollipop (interface provided)
    Lollipop,
}

impl RelationEnd {
    /// Integer code matching upstream's `relationType` enum values
    /// (`0=AGGREGATION, 1=EXTENSION, 2=COMPOSITION, 3=DEPENDENCY,
    /// 4=LOLLIPOP`). `-1` for none so it round-trips through `i64`.
    pub fn code(self) -> i32 {
        match self {
            Self::None => -1,
            Self::Aggregation => 0,
            Self::Extension => 1,
            Self::Composition => 2,
            Self::Dependency => 3,
            Self::Lollipop => 4,
        }
    }
}

/// Line type — solid (`--`) vs dotted (`..`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineType {
    Solid,
    Dotted,
}

impl LineType {
    /// `0=LINE, 1=DOTTED_LINE` — mirrors upstream enum.
    pub fn code(self) -> i32 {
        match self {
            Self::Solid => 0,
            Self::Dotted => 1,
        }
    }
}

/// Parsed relation. Mirrors upstream `ClassRelation`.
#[derive(Debug, Clone)]
pub struct ClassRelation {
    pub id1: String,
    pub id2: String,
    pub end1: RelationEnd,
    pub end2: RelationEnd,
    pub line: LineType,
    /// Multiplicity label on the id1 side (e.g. `"1"`, `"*"`).
    pub title1: String,
    /// Multiplicity label on the id2 side.
    pub title2: String,
    /// Main edge label (`: Cool` etc.). Empty when no label.
    pub title: String,
    pub style: Vec<String>,
}

/// Note attached via `note "..."` or `note for Foo "..."`.
#[derive(Debug, Clone)]
pub struct ClassNote {
    pub id: String,
    /// The class this note is attached to (`for Foo`), or empty.
    pub class_id: String,
    pub text: String,
    /// Insertion index — upstream threads this through `classDb.notes`.
    pub index: usize,
    /// Enclosing namespace id, if any.
    pub parent: Option<String>,
}

/// `classDef foo fill:#f9f` — a named reusable style set.
#[derive(Debug, Clone)]
pub struct StyleClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// `click`, `link`, `callback` directives. We collect them in parse
/// order rather than attaching to the class so the layout adapter can
/// replay them after every class is known.
#[derive(Debug, Clone)]
pub struct ClassInteractivity {
    pub class_id: String,
    pub kind: InteractivityKind,
    pub arg: String,
    pub arg2: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractivityKind {
    Callback,
    Link,
    ClickCallback,
    ClickHref,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_generic_plain() {
        assert_eq!(split_generic("Foo"), ("Foo", None));
    }

    #[test]
    fn split_generic_basic() {
        assert_eq!(split_generic("Foo~T~"), ("Foo", Some("T")));
    }

    #[test]
    fn class_mut_inserts_once() {
        let mut d = ClassDiagram::default();
        d.class_mut("Foo");
        d.class_mut("Foo");
        assert_eq!(d.classes.len(), 1);
    }

    #[test]
    fn visibility_glyphs() {
        assert_eq!(Visibility::Public.glyph(), "+");
        assert_eq!(Visibility::Private.glyph(), "-");
        assert_eq!(Visibility::Protected.glyph(), "#");
        assert_eq!(Visibility::Package.glyph(), "~");
        assert_eq!(Visibility::None.glyph(), "");
    }

    #[test]
    fn classifier_css() {
        assert_eq!(Classifier::Abstract.css(), "font-style:italic;");
        assert_eq!(Classifier::Static.css(), "text-decoration:underline;");
        assert_eq!(Classifier::None.css(), "");
    }

    #[test]
    fn relation_codes_match_upstream() {
        assert_eq!(RelationEnd::Aggregation.code(), 0);
        assert_eq!(RelationEnd::Extension.code(), 1);
        assert_eq!(RelationEnd::Composition.code(), 2);
        assert_eq!(RelationEnd::Dependency.code(), 3);
        assert_eq!(RelationEnd::Lollipop.code(), 4);
    }
}
