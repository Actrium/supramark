//! d2-ir: Intermediate representation for d2 diagrams.
//!
//! The IR resolves the flat AST into a tree of fields and edges,
//! handling underscore references, variable substitution, and
//! board overlays.
//!
//! Ported from Go `d2ir/d2ir.go` and `d2ir/compile.go`.
//! Simplified: no imports, no glob patterns.

use crate::ast::{self as ast};

// ---------------------------------------------------------------------------
// Core IR types
// ---------------------------------------------------------------------------

/// Scalar value wrapper.
#[derive(Debug, Clone)]
pub struct Scalar {
    pub value: ast::ScalarBox,
}

impl Scalar {
    pub fn scalar_string(&self) -> String {
        self.value.scalar_string()
    }
}

/// IR Array.
#[derive(Debug, Clone)]
pub struct Array {
    pub values: Vec<Value>,
}

/// Anything that can be a value: Scalar, Array, or Map.
#[derive(Debug, Clone)]
pub enum Value {
    Scalar(Scalar),
    Array(Array),
    Map(Map),
}

impl Value {
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_map_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_scalar(&self) -> Option<&Scalar> {
        match self {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&Array> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// Composite: Array or Map (not scalar).
#[derive(Debug, Clone)]
pub enum Composite {
    Array(Array),
    Map(Map),
}

impl Composite {
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Composite::Map(m) => Some(m),
            _ => None,
        }
    }
    pub fn as_map_mut(&mut self) -> Option<&mut Map> {
        match self {
            Composite::Map(m) => Some(m),
            _ => None,
        }
    }
}

/// A reference context capturing where a field/edge was defined.
#[derive(Debug, Clone)]
pub struct RefContext {
    pub key: ast::Key,
    pub edge_ast: Option<ast::Edge>,
    pub scope_map_idx: Option<usize>,
}

/// Reference to a field declaration.
#[derive(Debug, Clone)]
pub struct FieldReference {
    /// The string node that named this field.
    pub string: String,
    /// The full AST key path this reference came from (e.g. for `g.b`,
    /// even the FieldReference inside `g`'s field carries the entire
    /// `g.b` path). Mirrors Go d2ir.FieldReference.KeyPath.
    pub key_path: Option<ast::KeyPath>,
    /// Index into `key_path.path` for the segment this reference
    /// represents (0 for `g`, 1 for `b` in `g.b`). Mirrors Go
    /// d2ir.FieldReference.KeyPathIndex().
    pub key_path_index: usize,
    /// Whether this reference sets the primary value.
    pub primary: bool,
    /// Whether this reference was cloned from a variable/spread
    /// substitution and therefore points at the variable's declaration
    /// rather than the substitution site. Go's SortObjectsByAST skips
    /// sorting for such references, keeping insertion order.
    pub is_var: bool,
    pub context: RefContext,
}

/// Reference to an edge declaration.
#[derive(Debug, Clone)]
pub struct EdgeReference {
    pub context: RefContext,
}

/// A named field in the IR tree.
#[derive(Debug, Clone)]
pub struct Field {
    /// The name of the field (its key segment).
    pub name: String,
    /// Whether the name was unquoted in the source.
    pub name_is_unquoted: bool,
    /// The primary scalar value (label).
    pub primary: Option<Scalar>,
    /// The composite value (map or array).
    pub composite: Option<Composite>,
    /// All references to this field from the AST.
    pub references: Vec<FieldReference>,
    /// `Some(true)` when a `: suspend` declaration hid the field, `Some(false)`
    /// when `: unsuspend` restored it, `None` when untouched. Mirrors Go
    /// d2ir.Field.suspended. For glob templates (`**: suspend`), the value
    /// propagates through `apply_literal_star_template` to matched targets.
    pub suspended: Option<bool>,
}

impl Field {
    /// Get the primary scalar string, or None.
    pub fn primary_string(&self) -> Option<String> {
        self.primary.as_ref().map(|s| s.scalar_string())
    }

    /// Get the composite as a Map, if any.
    pub fn map(&self) -> Option<&Map> {
        self.composite.as_ref().and_then(|c| c.as_map())
    }

    /// Get the composite as a mutable Map.
    pub fn map_mut(&mut self) -> Option<&mut Map> {
        self.composite.as_mut().and_then(|c| c.as_map_mut())
    }

    /// Ensure this field has a Map composite. Returns mutable ref.
    pub fn ensure_map(&mut self) -> &mut Map {
        if self.composite.is_none() {
            self.composite = Some(Composite::Map(Map::new()));
        }
        match self.composite.as_mut().unwrap() {
            Composite::Map(m) => m,
            _ => panic!("expected map composite"),
        }
    }
}

/// Edge ID: identifies an edge by src/dst paths + arrows + index.
#[derive(Debug, Clone)]
pub struct EdgeID {
    pub src_path: Vec<String>,
    pub dst_path: Vec<String>,
    pub src_arrow: bool,
    pub dst_arrow: bool,
    pub index: Option<usize>,
    pub glob: bool,
}

/// Endpoint descriptors used when materialising a new edge in the IR. Bundles
/// the path/arrow/skip pairs that flow through `create_edge`/`create_edge_inner`.
struct EdgeEnds<'a> {
    src_path: &'a [String],
    dst_path: &'a [String],
    src_arrow: bool,
    dst_arrow: bool,
    src_skip: usize,
    dst_skip: usize,
}

/// Returns true if `seg` is a glob segment (`*` or `**`).
fn is_glob_segment(seg: &str) -> bool {
    seg == "*" || seg == "**"
}

/// Walk `map` and all descendant maps, appending `(map, edge_idx)` tuples
/// for every edge whose id matches `eid`. Used by glob edge application
/// so a pattern like `(** -> **)[*]` also hits edges that common-prefix
/// stripping placed in nested scopes.
fn collect_matching_edges_recursive(map: &mut Map, eid: &EdgeID, out: &mut Vec<(*mut Map, usize)>) {
    let map_ptr = map as *mut Map;
    for (idx, e) in map.edges.iter().enumerate() {
        if e.id.matches(eid) {
            out.push((map_ptr, idx));
        }
    }
    for f in &mut map.fields {
        if let Some(Composite::Map(ref mut inner)) = f.composite {
            collect_matching_edges_recursive(inner, eid, out);
        }
    }
}

/// Match a single path segment (glob-aware). When either side is `*` or
/// `**`, the segment pair is accepted — caller is responsible for path-
/// length handling. Used by `EdgeID::matches` and the glob-edge application
/// path.
fn segment_matches(lhs: &str, rhs: &str) -> bool {
    if is_glob_segment(lhs) || is_glob_segment(rhs) {
        return true;
    }
    lhs.eq_ignore_ascii_case(rhs)
}

/// Match two path segment sequences, honouring `*` (single segment) and
/// `**` (zero or more segments) wildcards on either side. Used by
/// `EdgeID::matches` for glob-edge lookup.
fn path_matches(a: &[String], b: &[String]) -> bool {
    // Fast path: no globs on either side ⇒ classic segment-wise equality.
    let has_glob_a = a.iter().any(|s| is_glob_segment(s));
    let has_glob_b = b.iter().any(|s| is_glob_segment(s));
    if !has_glob_a && !has_glob_b {
        if a.len() != b.len() {
            return false;
        }
        return a
            .iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y));
    }
    // Recursive matcher with `**` = zero-or-more segments.
    fn rec(a: &[String], b: &[String]) -> bool {
        if a.is_empty() && b.is_empty() {
            return true;
        }
        if a.is_empty() {
            return b.iter().all(|s| s == "**");
        }
        if b.is_empty() {
            return a.iter().all(|s| s == "**");
        }
        let (ah, at) = (&a[0], &a[1..]);
        let (bh, bt) = (&b[0], &b[1..]);
        // `**` on either side: try matching zero or more.
        if ah == "**" {
            // Skip the `**` (match zero segments) OR consume one segment of b.
            return rec(at, b) || rec(a, bt);
        }
        if bh == "**" {
            return rec(a, bt) || rec(at, b);
        }
        if !segment_matches(ah, bh) {
            return false;
        }
        rec(at, bt)
    }
    rec(a, b)
}

impl EdgeID {
    /// Check if two EdgeIDs match (for lookup).
    ///
    /// When either side contains glob segments (`*` or `**`) they act as
    /// wildcards: `*` matches any single path segment and `**` matches
    /// zero or more segments. This mirrors the glob expansion in Go's
    /// `d2ir` where `(* -> *)[*]` and `(** -> **)[*]` style declarations
    /// apply to every matching edge.
    pub fn matches(&self, other: &EdgeID) -> bool {
        if self.index.is_some() && other.index.is_some() && self.index != other.index {
            return false;
        }
        if self.src_arrow != other.src_arrow {
            return false;
        }
        if self.dst_arrow != other.dst_arrow {
            return false;
        }
        if !path_matches(&self.src_path, &other.src_path) {
            return false;
        }
        if !path_matches(&self.dst_path, &other.dst_path) {
            return false;
        }
        true
    }
}

/// An edge in the IR.
#[derive(Debug, Clone)]
pub struct IREdge {
    pub id: EdgeID,
    pub primary: Option<Scalar>,
    pub map: Option<Map>,
    pub references: Vec<EdgeReference>,
    /// Suspension state set by `: suspend` / `: unsuspend` declarations.
    /// Mirrors Go d2ir.Edge.suspended. Unsuspending an edge also lifts
    /// suspension from its src/dst objects and their ancestors.
    pub suspended: Option<bool>,
}

/// A single `&attr: value` (or `!&attr: value`) filter entry recorded
/// on a map. Used by the compiler to narrow `**` / `*` glob expansion
/// to only fields whose attribute matches (or does not match, when
/// `negate` is set).
#[derive(Debug, Clone)]
pub struct Filter {
    /// Dotted attribute path, e.g. `shape`, `class`, `style.fill`, `label`.
    pub attr: Vec<String>,
    /// Expected scalar value (lowercase-compared).
    pub value: String,
    /// Set for `!&` — negated filters.
    pub negate: bool,
}

/// The resolved map of fields and edges.
#[derive(Debug, Clone)]
pub struct Map {
    pub fields: Vec<Field>,
    pub edges: Vec<IREdge>,
    /// `&attr: value` filter entries declared directly inside this map.
    /// When the map is used as a `**` / `*` glob template, the compiler
    /// only applies its non-filter fields to targets that satisfy every
    /// filter listed here.
    pub filters: Vec<Filter>,
}

impl Map {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            edges: Vec::new(),
            filters: Vec::new(),
        }
    }

    /// Look up a field by name (case-insensitive for user fields, exact for reserved).
    pub fn get_field(&self, name: &str) -> Option<&Field> {
        let lower = name.to_lowercase();
        let is_reserved = ast::RESERVED_KEYWORDS.contains(lower.as_str());
        for f in &self.fields {
            if is_reserved {
                // Reserved keywords match case-insensitively, but only if unquoted
                if f.name.to_lowercase() == lower && f.name_is_unquoted {
                    return Some(f);
                }
            } else if f.name.eq_ignore_ascii_case(name) {
                return Some(f);
            }
        }
        None
    }

    /// Look up a field by name (mutable).
    pub fn get_field_mut(&mut self, name: &str) -> Option<&mut Field> {
        let lower = name.to_lowercase();
        let is_reserved = ast::RESERVED_KEYWORDS.contains(lower.as_str());
        for f in &mut self.fields {
            if is_reserved {
                if f.name.to_lowercase() == lower && f.name_is_unquoted {
                    return Some(f);
                }
            } else if f.name.eq_ignore_ascii_case(name) {
                return Some(f);
            }
        }
        None
    }

    /// Multi-segment field lookup: `get_field_path(&["a", "b", "c"])`.
    pub fn get_field_path(&self, path: &[&str]) -> Option<&Field> {
        if path.is_empty() {
            return None;
        }
        let f = self.get_field(path[0])?;
        if path.len() == 1 {
            return Some(f);
        }
        f.map()?.get_field_path(&path[1..])
    }

    /// Get edges matching a given EdgeID.
    pub fn get_edges(&self, eid: &EdgeID) -> Vec<&IREdge> {
        self.edges.iter().filter(|e| e.id.matches(eid)).collect()
    }

    /// Delete a field by name. Returns the removed field if found.
    pub fn delete_field(&mut self, name: &str) -> Option<Field> {
        let idx = self
            .fields
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(name))?;
        Some(self.fields.remove(idx))
    }

    /// Delete an edge by EdgeID. Returns the removed edge if found.
    pub fn delete_edge(&mut self, eid: &EdgeID) -> Option<IREdge> {
        let idx = self.edges.iter().position(|e| e.id.matches(eid))?;
        Some(self.edges.remove(idx))
    }

    /// Copy the map without layers/scenarios/steps fields.
    /// Mirrors Go d2ir.Map.CopyBase.
    pub fn copy_base(&self) -> Map {
        let mut m = self.clone();
        m.delete_field("layers");
        m.delete_field("scenarios");
        m.delete_field("steps");
        // Also remove "label" as Go does in overlay
        m.delete_field("label");
        m
    }
}

impl Default for Map {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Overlay / merge operations (from merge.go)
// ---------------------------------------------------------------------------

/// Overlay `overlay` map onto `base` map.
pub fn overlay_map(base: &mut Map, overlay: &Map) {
    for of in &overlay.fields {
        if let Some(bf) = base.get_field_mut(&of.name) {
            overlay_field(bf, of);
        } else {
            base.fields.push(of.clone());
        }
    }
    for oe in &overlay.edges {
        let existing: Vec<usize> = base
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.id.matches(&oe.id))
            .map(|(i, _)| i)
            .collect();
        if existing.is_empty() {
            base.edges.push(oe.clone());
        } else {
            let idx = existing[0];
            overlay_edge(&mut base.edges[idx], oe);
        }
    }
}

fn expand_substitution(base: &mut Map, resolved: &Map, placeholder_idx: usize) {
    let mut insert_at = placeholder_idx;
    for of in &resolved.fields {
        if let Some(bf) = base.get_field_mut(&of.name) {
            overlay_field(bf, of);
        } else {
            let mut cloned = of.clone();
            mark_refs_as_var(&mut cloned);
            base.fields.insert(insert_at, cloned);
            insert_at += 1;
        }
    }

    // Match Go d2ir.ExpandSubstitution: edges are appended, not inserted in place.
    for oe in &resolved.edges {
        let existing: Vec<usize> = base
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.id.matches(&oe.id))
            .map(|(i, _)| i)
            .collect();
        if existing.is_empty() {
            base.edges.push(oe.clone());
        } else {
            let idx = existing[0];
            overlay_edge(&mut base.edges[idx], oe);
        }
    }
}

/// Recursively mark all FieldReferences under this field (and its
/// nested composites) as is_var=true. Matches Go d2ir.Copy behavior
/// where spread-expanded fields track that their references point at
/// the variable, not the substitution location.
fn mark_refs_as_var(f: &mut Field) {
    for r in &mut f.references {
        r.is_var = true;
    }
    if let Some(ref mut c) = f.composite {
        match c {
            Composite::Map(m) => {
                for sub in &mut m.fields {
                    mark_refs_as_var(sub);
                }
                for e in &mut m.edges {
                    for er in &mut e.references {
                        let _ = er;
                    }
                }
            }
            Composite::Array(_) => {}
        }
    }
}

fn overlay_field(bf: &mut Field, of: &Field) {
    if of.primary.is_some() {
        bf.primary = of.primary.clone();
    }
    if let Some(ref oc) = of.composite {
        match (&mut bf.composite, oc) {
            (Some(Composite::Map(bm)), Composite::Map(om)) => {
                overlay_map(bm, om);
            }
            _ => {
                bf.composite = Some(oc.clone());
            }
        }
    }
    bf.references.extend(of.references.iter().cloned());
}

fn overlay_edge(be: &mut IREdge, oe: &IREdge) {
    if oe.primary.is_some() {
        be.primary = oe.primary.clone();
    }
    if let Some(ref om) = oe.map {
        match &mut be.map {
            Some(bm) => {
                overlay_map(bm, om);
            }
            None => {
                be.map = Some(om.clone());
            }
        }
    }
    be.references.extend(oe.references.iter().cloned());
}

// ---------------------------------------------------------------------------
// AST -> IR Compiler
// ---------------------------------------------------------------------------

/// Compile errors.
#[derive(Debug, Clone, Default)]
pub struct CompileError {
    pub errors: Vec<ast::Error>,
}

impl CompileError {
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, e) in self.errors.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{}", e)?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}

/// A deferred glob-edge application: the `Key` holds a pattern like
/// `(** -> **)[*].style.X: v`, `scope_map` is the IR map it was declared
/// in, and `edge_idx` is its index inside `Key.edges`. After the first
/// compile pass, `apply_pending_globs` re-visits each entry so the
/// pattern hits edges that were declared textually after it.
struct PendingGlob {
    scope_map: *mut Map,
    key: ast::Key,
    edge_idx: usize,
}

struct Compiler {
    errors: Vec<ast::Error>,
    /// Stack of parent Maps. The top of stack is the parent of the current
    /// scope, enabling `_.foo` (parent reference) resolution in paths.
    /// Empty when compiling at root scope.
    scope_stack: Vec<*mut Map>,
    /// Parallel stack of scope field names (the names of fields traversed
    /// to enter each nested map). Used to prepend scope prefixes when
    /// placing edges at a higher scope via `_` references.
    scope_name_stack: Vec<String>,
    /// Glob-edge style declarations that must be re-applied once the whole
    /// map has been compiled, so they hit edges declared later in source
    /// order. Mirrors Go's `globContexts` / `lazyGlobBeingApplied` loop.
    pending_globs: Vec<PendingGlob>,
}

impl Compiler {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            scope_stack: Vec::new(),
            scope_name_stack: Vec::new(),
            pending_globs: Vec::new(),
        }
    }

    /// Resolve a KeyPath that may start with `_` (parent references).
    /// Returns the effective scope map and the tail path (with `_`s stripped).
    /// If `_` is used without an available parent, returns an error and
    /// falls back to the current scope with the original path.
    ///
    /// # Safety
    /// The returned map pointer is only valid while the scope stack remains
    /// stable. Callers must not modify the stack while using the pointer.
    unsafe fn resolve_underscore_scope<'b>(
        &mut self,
        current: &mut Map,
        path: &'b [ast::StringBox],
    ) -> (*mut Map, &'b [ast::StringBox]) {
        let mut scope: *mut Map = current as *mut Map;
        let mut rest = path;
        let mut stack_idx = self.scope_stack.len();
        while let Some(first) = rest.first() {
            if first.scalar_string() == "_" && first.as_unquoted().is_some() {
                if stack_idx == 0 {
                    self.errorf(
                        first.get_range(),
                        "invalid underscore: no parent".to_owned(),
                    );
                    return (current as *mut Map, path);
                }
                stack_idx -= 1;
                scope = self.scope_stack[stack_idx];
                rest = &rest[1..];
                if rest.is_empty() {
                    self.errorf(
                        first.get_range(),
                        "field key must contain more than underscores".to_owned(),
                    );
                    return (current as *mut Map, path);
                }
            } else {
                break;
            }
        }
        (scope, rest)
    }

    fn errorf(&mut self, range: &ast::Range, msg: String) {
        self.errors.push(ast::Error {
            range: range.clone(),
            message: msg,
        });
    }
}

/// Compile an AST Map into an IR Map.
pub fn compile(ast_map: &ast::Map) -> Result<Map, CompileError> {
    let mut c = Compiler::new();
    let mut m = Map::new();

    c.compile_map(&mut m, ast_map);
    c.compile_substitutions(&mut m, &[]);
    // Re-apply glob edge declarations now that every edge has been created.
    // In Go this is done implicitly via `lazyGlobBeingApplied`; we mirror
    // it with an explicit deferred queue.
    c.apply_pending_globs();

    if c.errors.is_empty() {
        Ok(m)
    } else {
        Err(CompileError { errors: c.errors })
    }
}

impl Compiler {
    fn compile_map(&mut self, dst: &mut Map, ast_map: &ast::Map) {
        for node in &ast_map.nodes {
            match node {
                ast::MapNode::Key(key) => {
                    self.compile_key(dst, key);
                }
                ast::MapNode::Substitution(sub) => {
                    // Create placeholder field for later substitution resolution
                    let f = Field {
                        name: String::new(),
                        name_is_unquoted: true,
                        primary: Some(Scalar {
                            value: ast::ScalarBox::UnquotedString(ast::UnquotedString {
                                range: sub.range.clone(),
                                value: vec![ast::InterpolationBox {
                                    string: None,
                                    string_raw: None,
                                    substitution: Some(sub.clone()),
                                }],
                                pattern: None,
                            }),
                        }),
                        composite: None,
                        references: Vec::new(),
                        suspended: None,
                    };
                    dst.fields.push(f);
                }
                _ => {} // Comments, imports (skipped)
            }
        }
    }

    fn compile_key(&mut self, dst: &mut Map, key: &ast::Key) {
        // Ampersand / filter keys: record as Filter on the enclosing map
        // rather than as a regular field. The compiler consults these when
        // expanding `**` (and `*`) templates so only fields whose attribute
        // matches (or doesn't match, for `!&`) receive the template values.
        if key.ampersand || key.not_ampersand {
            if let Some(ref kp) = key.key
                && !kp.path.is_empty()
            {
                let attr: Vec<String> = kp
                    .path
                    .iter()
                    .map(|sb| sb.scalar_string().to_string())
                    .collect();
                let value = if let Some(ref p) = key.primary {
                    p.scalar_string().to_string()
                } else if let Some(ref v) = key.value {
                    v.scalar_box()
                        .map(|sb| sb.scalar_string().to_string())
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                dst.filters.push(Filter {
                    attr,
                    value,
                    negate: key.not_ampersand,
                });
            }
            return;
        }

        if key.edges.is_empty() {
            // Field key
            if let Some(ref kp) = key.key {
                self.compile_field(dst, kp, key);
            }
        } else {
            // Edge key
            self.compile_edges(dst, key);
        }
    }

    fn compile_field(&mut self, dst: &mut Map, kp: &ast::KeyPath, key: &ast::Key) {
        if kp.path.is_empty() {
            return;
        }

        // Resolve any leading `_` (parent scope references). Matches Go
        // d2ir.Map.EnsureField, which pops up to ParentMap for each leading
        // underscore.
        let (scope_ptr, rest_path) = unsafe { self.resolve_underscore_scope(dst, &kp.path) };
        if rest_path.is_empty() {
            return;
        }
        let path: Vec<&ast::StringBox> = rest_path.iter().collect();

        // Walk/create the field path
        let mut cur_map: *mut Map = scope_ptr;
        for (i, sb) in path.iter().enumerate() {
            let name = sb.scalar_string().to_string();
            let is_unquoted = matches!(sb, ast::StringBox::Unquoted(_));
            let lower = name.to_lowercase();

            // Validate reserved keywords
            if ast::RESERVED_KEYWORDS.contains(lower.as_str())
                && is_unquoted
                && !ast::COMPOSITE_RESERVED_KEYWORDS.contains(lower.as_str())
                && i < path.len() - 1
            {
                self.errorf(
                    sb.get_range(),
                    format!("\"{}\" must be the last part of the key", lower),
                );
                return;
            }

            // Safety: we only have one mutable reference at a time
            let m = unsafe { &mut *cur_map };

            if i == path.len() - 1 {
                // Terminal: ensure field exists, record the reference, then
                // apply the value.
                let f = self.ensure_field(m, &name, is_unquoted);
                f.references.push(FieldReference {
                    string: name.clone(),
                    key_path: Some(kp.clone()),
                    key_path_index: i,
                    primary: true,
                    is_var: false,
                    context: RefContext {
                        key: key.clone(),
                        edge_ast: None,
                        scope_map_idx: None,
                    },
                });
                self.apply_field_value(f, key, cur_map);
                return;
            }

            // Non-terminal: ensure field, record the reference, then descend
            // into its map.
            let f = self.ensure_field(m, &name, is_unquoted);
            f.references.push(FieldReference {
                string: name.clone(),
                key_path: Some(kp.clone()),
                key_path_index: i,
                primary: false,
                is_var: false,
                context: RefContext {
                    key: key.clone(),
                    edge_ast: None,
                    scope_map_idx: None,
                },
            });
            if f.composite.is_none() {
                f.composite = Some(Composite::Map(Map::new()));
            }
            match f.composite.as_mut() {
                Some(Composite::Map(inner)) => {
                    cur_map = inner as *mut Map;
                }
                Some(Composite::Array(_)) => {
                    self.errorf(sb.get_range(), "cannot index into array".to_string());
                    return;
                }
                None => unreachable!(),
            }
        }
    }

    fn ensure_field<'a>(&mut self, m: &'a mut Map, name: &str, is_unquoted: bool) -> &'a mut Field {
        let lower = name.to_lowercase();
        let is_reserved = ast::RESERVED_KEYWORDS.contains(lower.as_str());

        // `**` glob templates are never deduplicated: each occurrence in the
        // source carries its own filter set and its own RHS body, which must
        // be applied independently at expansion time. Merging them (the
        // default dedup behaviour below) would accumulate filters/attrs from
        // unrelated blocks and produce wrong matches. Mirrors Go d2ir where
        // each `**: {...}` key triggers a fresh `multiGlob`/`_compileField`
        // pass against the current map rather than writing into a shared
        // `**` field. `*` is left shared for now since its semantics are
        // narrower (single-level) and no failing fixture currently exercises
        // multi-block `*` globs with filters.
        if is_unquoted && name == "**" {
            let f = Field {
                name: name.to_string(),
                name_is_unquoted: is_unquoted,
                primary: None,
                composite: None,
                references: Vec::new(),
                suspended: None,
            };
            m.fields.push(f);
            return m.fields.last_mut().unwrap();
        }

        // Look for existing field
        let existing_idx = m.fields.iter().position(|f| {
            if is_reserved {
                f.name.to_lowercase() == lower && f.name_is_unquoted == is_unquoted
            } else {
                f.name.eq_ignore_ascii_case(name)
            }
        });

        if let Some(idx) = existing_idx {
            return &mut m.fields[idx];
        }

        // Create new field
        let f = Field {
            name: name.to_string(),
            name_is_unquoted: is_unquoted,
            primary: None,
            composite: None,
            references: Vec::new(),
            suspended: None,
        };
        m.fields.push(f);
        m.fields.last_mut().unwrap()
    }

    fn apply_field_value(&mut self, f: &mut Field, key: &ast::Key, parent_map_ptr: *mut Map) {
        // Check for null -> delete (simplified: just skip)
        if let Some(ref v) = key.value
            && matches!(v, ast::ValueBox::Null(_))
        {
            return;
        }
        if let Some(ref p) = key.primary
            && matches!(p, ast::ScalarBox::Null(_))
        {
            return;
        }

        // Primary value
        if let Some(ref primary) = key.primary {
            if let ast::ScalarBox::Suspension(s) = primary {
                // `**: suspend` / `*: unsuspend` record the suspension state
                // on the field so the graph-level glob expansion can carry
                // it into matched targets. Mirrors Go d2ir's
                // `f.suspended = refctx.Key.Primary.Suspension.Value`.
                f.suspended = Some(s.value);
            } else {
                f.primary = Some(Scalar {
                    value: primary.clone(),
                });
            }
        }

        // Value
        if let Some(ref val) = key.value {
            match val {
                ast::ValueBox::Array(arr) => {
                    let mut ir_arr = Array { values: Vec::new() };
                    self.compile_array(&mut ir_arr, arr);
                    f.composite = Some(Composite::Array(ir_arr));
                }
                ast::ValueBox::Map(m) => {
                    if f.composite.is_none() || !matches!(f.composite, Some(Composite::Map(_))) {
                        f.composite = Some(Composite::Map(Map::new()));
                    }
                    // Save the outer scope (dst's parent chain + dst) on the
                    // stack so nested `_.x` references can resolve upward.
                    // Also record the field name so edge paths that move
                    // up via `_` can rebuild the crossed scope prefix.
                    let outer_ptr: *mut Map = parent_map_ptr;
                    self.scope_stack.push(outer_ptr);
                    self.scope_name_stack.push(f.name.clone());
                    if let Some(Composite::Map(ref mut fm)) = f.composite {
                        self.compile_map(fm, m);
                    }
                    self.scope_name_stack.pop();
                    self.scope_stack.pop();
                }
                ast::ValueBox::Null(_) => {}
                ast::ValueBox::Suspension(s) => {
                    f.suspended = Some(s.value);
                }
                _ => {
                    // Scalar value
                    if let Some(sb) = val.scalar_box() {
                        if let ast::ScalarBox::Suspension(s) = sb {
                            f.suspended = Some(s.value);
                        } else {
                            f.primary = Some(Scalar { value: sb });
                        }
                    }
                }
            }
        }
    }

    fn compile_edges(&mut self, dst: &mut Map, key: &ast::Key) {
        // If there's a common key prefix, ensure those fields first
        if let Some(ref common_kp) = key.key {
            // Resolve leading `_`s to parent scope before walking
            let (scope_ptr, rest) = unsafe { self.resolve_underscore_scope(dst, &common_kp.path) };
            let path: Vec<String> = rest
                .iter()
                .map(|sb| sb.scalar_string().to_string())
                .collect();
            let scope = unsafe { &mut *scope_ptr };
            let inner = self.ensure_field_path(scope, &path);
            self.compile_edges_inner(inner, key);
        } else {
            self.compile_edges_inner(dst, key);
        }
    }

    fn ensure_field_path<'a>(&mut self, dst: &'a mut Map, path: &[String]) -> &'a mut Map {
        self.ensure_field_path_with_refs(dst, path, None, None, 0)
    }

    /// Like `ensure_field_path_with_refs`, but skip recording a new
    /// FieldReference for the first `skip` path components. Used when the
    /// caller has prepended scope names to reconstruct an absolute path
    /// after resolving `_` — those prepended names already have their own
    /// references from the containing scope, so recording them again (with
    /// AST indices that don't match the original KeyPath) pollutes the
    /// "first reference" used by SortObjectsByAST.
    fn ensure_field_path_with_refs_skip<'a>(
        &mut self,
        dst: &'a mut Map,
        path: &[String],
        kp: Option<&ast::KeyPath>,
        key: Option<&ast::Key>,
        kp_offset: usize,
        skip: usize,
    ) -> &'a mut Map {
        let mut cur = dst as *mut Map;
        for (i, name) in path.iter().enumerate() {
            let m = unsafe { &mut *cur };
            let f = self.ensure_field(m, name, true);
            if i >= skip
                && let (Some(kp), Some(key)) = (kp, key)
            {
                f.references.push(FieldReference {
                    string: name.clone(),
                    key_path: Some(kp.clone()),
                    key_path_index: kp_offset + (i - skip),
                    primary: false,
                    is_var: false,
                    context: RefContext {
                        key: key.clone(),
                        edge_ast: None,
                        scope_map_idx: None,
                    },
                });
            }
            if f.composite.is_none() {
                f.composite = Some(Composite::Map(Map::new()));
            }
            match f.composite.as_mut().unwrap() {
                Composite::Map(inner) => {
                    cur = inner as *mut Map;
                }
                _ => break,
            }
        }
        unsafe { &mut *cur }
    }

    /// Variant that records FieldReferences along the path. `kp` is the
    /// originating AST KeyPath (e.g. the src/dst of an edge), `key` is its
    /// containing AST Key, and `kp_offset` is the index in `kp.path` where
    /// `path[0]` lives. Used by edge processing so the new fields end up
    /// with reference info that points back to the right AST nodes.
    fn ensure_field_path_with_refs<'a>(
        &mut self,
        dst: &'a mut Map,
        path: &[String],
        kp: Option<&ast::KeyPath>,
        key: Option<&ast::Key>,
        kp_offset: usize,
    ) -> &'a mut Map {
        let mut cur = dst as *mut Map;
        for (i, name) in path.iter().enumerate() {
            let m = unsafe { &mut *cur };
            let f = self.ensure_field(m, name, true);
            if let (Some(kp), Some(key)) = (kp, key) {
                f.references.push(FieldReference {
                    string: name.clone(),
                    key_path: Some(kp.clone()),
                    key_path_index: kp_offset + i,
                    primary: false,
                    is_var: false,
                    context: RefContext {
                        key: key.clone(),
                        edge_ast: None,
                        scope_map_idx: None,
                    },
                });
            }
            if f.composite.is_none() {
                f.composite = Some(Composite::Map(Map::new()));
            }
            match f.composite.as_mut().unwrap() {
                Composite::Map(inner) => {
                    cur = inner as *mut Map;
                }
                _ => break,
            }
        }
        unsafe { &mut *cur }
    }

    fn compile_edges_inner(&mut self, dst: &mut Map, key: &ast::Key) {
        for (i, ast_edge) in key.edges.iter().enumerate() {
            // Count leading `_`s in src/dst and strip them. The edge is
            // placed in the scope max(src_up, dst_up) levels above `dst`.
            // Paths that were NOT shifted get the intervening scope names
            // prepended (matches Go: `a: { f.g -> _.s.n }` becomes edge
            // `a.f.g -> s.n` at root).
            let count_underscores = |path: &[ast::StringBox]| -> usize {
                path.iter()
                    .take_while(|sb| sb.scalar_string() == "_" && sb.as_unquoted().is_some())
                    .count()
            };
            let src_up = ast_edge
                .src
                .as_ref()
                .map(|kp| count_underscores(&kp.path))
                .unwrap_or(0);
            let dst_up = ast_edge
                .dst
                .as_ref()
                .map(|kp| count_underscores(&kp.path))
                .unwrap_or(0);
            let edge_up = src_up.max(dst_up);
            if edge_up > self.scope_stack.len() {
                // More `_`s than available parents: report error and skip
                if let Some(first_kp) = ast_edge.src.as_ref().or(ast_edge.dst.as_ref())
                    && let Some(first_underscore) =
                        first_kp.path.iter().find(|sb| sb.scalar_string() == "_")
                {
                    self.errorf(
                        first_underscore.get_range(),
                        "invalid underscore: no parent".to_owned(),
                    );
                }
                continue;
            }
            // Build src/dst relative paths for the edge, rooted at edge_scope.
            // For a path with N underscores, its tail starts at (edge_up - N)
            // levels below edge_scope; we prepend the names of those levels.
            let current_tail: Vec<String> = self
                .scope_stack
                .iter()
                .rev()
                .take(edge_up)
                .map(|p| unsafe { &**p })
                .filter_map(|_| None::<String>) // placeholder; see below
                .collect();
            // We don't track scope names in the stack, so fall back: for
            // the shifted side, paths have `_`s stripped; for the non-shifted
            // side, prepend the last `edge_up - their_up` scope names.
            // Since we don't have names, derive them from the KeyPath trail
            // in the current compile context via key.edges[i].src/dst range —
            // too brittle; instead, accept the limitation that cross-scope
            // prefix is not reconstructed, and only apply scope_shift when
            // BOTH sides share the same underscore count (most common case
            // in the dagre_spacing corpus where edges like `a -> b` at
            // various levels use `_` consistently).
            let _ = current_tail;
            let build_path = |opt: Option<&ast::KeyPath>, ups: usize| -> Vec<String> {
                match opt {
                    Some(kp) => kp
                        .path
                        .iter()
                        .skip(ups)
                        .map(|sb| sb.scalar_string().to_string())
                        .collect(),
                    None => Vec::new(),
                }
            };
            let src_tail = build_path(ast_edge.src.as_ref(), src_up);
            let dst_tail = build_path(ast_edge.dst.as_ref(), dst_up);
            // If one side was shifted more than the other, prepend the
            // scope names the less-shifted side would otherwise cross.
            // Scope names are recovered from the most-recent field names
            // the compiler placed on the stack when descending. For now,
            // we use ids_to_edge_scope to capture this info.
            let shift_prefix = |side_up: usize| -> Vec<String> {
                // Need scope_name_stack (parallel to scope_stack). See
                // scope_name_stack field on Compiler.
                let diff = edge_up - side_up;
                if diff == 0 {
                    Vec::new()
                } else {
                    self.scope_name_stack
                        .iter()
                        .rev()
                        .take(diff)
                        .rev()
                        .cloned()
                        .collect()
                }
            };
            let mut src_path: Vec<String> = shift_prefix(src_up);
            src_path.extend(src_tail);
            let mut dst_path: Vec<String> = shift_prefix(dst_up);
            dst_path.extend(dst_tail);

            // Redirect `dst` (the map the edge gets stored in) to the
            // scope `edge_up` levels above the current dst.
            let edge_scope_ptr: *mut Map = if edge_up == 0 {
                dst as *mut Map
            } else {
                self.scope_stack[self.scope_stack.len() - edge_up]
            };
            let dst: &mut Map = unsafe { &mut *edge_scope_ptr };

            let src_arrow = ast_edge.src_arrow == "<";
            let dst_arrow = ast_edge.dst_arrow == ">";

            // Check for indexed/glob edge reference
            let has_index = key
                .edge_index
                .as_ref()
                .map(|ei| ei.int.is_some() || ei.glob)
                .unwrap_or(false);

            if has_index {
                // Look up existing edge by index
                let edge_index = key
                    .edge_index
                    .as_ref()
                    .and_then(|ei| ei.int.map(|v| v as usize));
                let eid = EdgeID {
                    src_path: src_path.clone(),
                    dst_path: dst_path.clone(),
                    src_arrow,
                    dst_arrow,
                    index: edge_index,
                    glob: key.edge_index.as_ref().map(|ei| ei.glob).unwrap_or(false),
                };

                // Collect matches. For glob edges with `**` in either path
                // segment, also walk descendant maps — the edge may have
                // been stored at a nested scope (e.g. common-prefix
                // stripping placed `Libreria.t1 -> Libreria.t1` in
                // `Libreria`'s map, not root). Non-glob indexed edges must
                // exist at the exact scope, so we skip recursion there.
                let has_double_glob =
                    src_path.iter().any(|s| s == "**") || dst_path.iter().any(|s| s == "**");
                let mut match_locations: Vec<(*mut Map, usize)> = Vec::new();
                if eid.glob && has_double_glob {
                    collect_matching_edges_recursive(dst, &eid, &mut match_locations);
                } else {
                    let dst_ptr = dst as *mut Map;
                    for (idx, e) in dst.edges.iter().enumerate() {
                        if e.id.matches(&eid) {
                            match_locations.push((dst_ptr, idx));
                        }
                    }
                }

                // Record glob edges so they are re-applied after every
                // edge has been created — otherwise a pattern declared
                // before its target edges (e.g. `(* -> *)[*].style…` at
                // the top of a file) would silently do nothing.
                if eid.glob {
                    self.pending_globs.push(PendingGlob {
                        scope_map: edge_scope_ptr,
                        key: key.clone(),
                        edge_idx: i,
                    });
                }

                if match_locations.is_empty() {
                    if !eid.glob {
                        self.errorf(&ast_edge.range, "indexed edge does not exist".to_string());
                    }
                    continue;
                }

                for (map_ptr, eidx) in &match_locations {
                    // SAFETY: we hold an exclusive borrow on `dst`'s tree;
                    // each location is a distinct (map, edge_idx) pair and
                    // we only touch one at a time. The compiler cannot track
                    // this, so we use raw pointers — matching the pattern
                    // already used by `scope_stack` / `ensure_field_path`.
                    let m = unsafe { &mut **map_ptr };
                    let e = &mut m.edges[*eidx];
                    e.references.push(EdgeReference {
                        context: RefContext {
                            key: key.clone(),
                            edge_ast: Some(ast_edge.clone()),
                            scope_map_idx: None,
                        },
                    });
                    self.apply_edge_value(e, key, i);
                }
            } else {
                // Create new edge. For paths shifted up via `_`, the
                // prepended scope names already have their own references
                // from the containing scope's AST; don't add spurious
                // references pointing at the wrong AST positions.
                let src_skip = edge_up.saturating_sub(src_up);
                let dst_skip = edge_up.saturating_sub(dst_up);
                self.create_edge(
                    dst,
                    key,
                    i,
                    EdgeEnds {
                        src_path: &src_path,
                        dst_path: &dst_path,
                        src_arrow,
                        dst_arrow,
                        src_skip,
                        dst_skip,
                    },
                );
            }
        }
    }

    fn create_edge(&mut self, dst: &mut Map, key: &ast::Key, edge_idx: usize, ends: EdgeEnds) {
        let EdgeEnds {
            src_path,
            dst_path,
            src_arrow,
            dst_arrow,
            src_skip,
            dst_skip,
        } = ends;
        // Resolve common prefix
        let mut common_len = 0;
        for (a, b) in src_path.iter().zip(dst_path.iter()) {
            if a.eq_ignore_ascii_case(b) && src_path.len() > 1 && dst_path.len() > 1 {
                common_len += 1;
            } else {
                break;
            }
        }

        if common_len > 0 {
            let common = &src_path[..common_len];
            let inner_src = &src_path[common_len..];
            let inner_dst = &dst_path[common_len..];

            // Ensure common path fields exist. Record references to the
            // original edge src/dst keypaths so shapes like the common-
            // prefix field "a" in `a.b -> a.c` get first-references that
            // match Go's sort_objects_by_ast (pointing at path[0]="a").
            let ast_edge_for_paths = key.edges.get(edge_idx);
            let src_kp = ast_edge_for_paths.and_then(|e| e.src.as_ref());
            let scope = self.ensure_field_path_with_refs_skip(
                dst,
                common,
                src_kp,
                Some(key),
                0,
                src_skip.min(common_len),
            );
            self.create_edge_inner(
                scope,
                key,
                edge_idx,
                EdgeEnds {
                    src_path: inner_src,
                    dst_path: inner_dst,
                    src_arrow,
                    dst_arrow,
                    src_skip: src_skip.saturating_sub(common_len),
                    dst_skip: dst_skip.saturating_sub(common_len),
                },
                common_len,
            );
        } else {
            self.create_edge_inner(dst, key, edge_idx, ends, 0);
        }
    }

    fn create_edge_inner(
        &mut self,
        dst: &mut Map,
        key: &ast::Key,
        edge_idx: usize,
        ends: EdgeEnds,
        kp_offset: usize,
    ) {
        let EdgeEnds {
            src_path,
            dst_path,
            src_arrow,
            dst_arrow,
            src_skip,
            dst_skip,
        } = ends;
        // Ensure src and dst fields exist, recording the AST KeyPath for
        // each new field reference. `kp_offset` is the number of path
        // segments (common prefix) already consumed from the AST KeyPath
        // by the caller; when walking `src_path` at index i, the matching
        // AST StringBox sits at `kp.path[kp_offset + i]`.
        let ast_edge_for_paths = key.edges.get(edge_idx);
        if !src_path.is_empty() {
            let src_kp = ast_edge_for_paths.and_then(|e| e.src.as_ref());
            self.ensure_field_path_with_refs_skip(
                dst,
                src_path,
                src_kp,
                Some(key),
                kp_offset,
                src_skip,
            );
        }
        if !dst_path.is_empty() {
            let dst_kp = ast_edge_for_paths.and_then(|e| e.dst.as_ref());
            self.ensure_field_path_with_refs_skip(
                dst,
                dst_path,
                dst_kp,
                Some(key),
                kp_offset,
                dst_skip,
            );
        }

        // Count existing edges with same src/dst/arrows (to compute index)
        let match_eid = EdgeID {
            src_path: src_path.to_vec(),
            dst_path: dst_path.to_vec(),
            src_arrow,
            dst_arrow,
            index: None,
            glob: true,
        };
        let count = dst
            .edges
            .iter()
            .filter(|e| e.id.matches(&match_eid))
            .count();

        let eid = EdgeID {
            src_path: src_path.to_vec(),
            dst_path: dst_path.to_vec(),
            src_arrow,
            dst_arrow,
            index: Some(count),
            glob: false,
        };

        let ast_edge = key.edges.get(edge_idx).cloned();

        let mut e = IREdge {
            id: eid,
            primary: None,
            map: None,
            references: vec![EdgeReference {
                context: RefContext {
                    key: key.clone(),
                    edge_ast: ast_edge,
                    scope_map_idx: None,
                },
            }],
            suspended: None,
        };

        self.apply_edge_value(&mut e, key, edge_idx);
        dst.edges.push(e);
    }

    fn apply_edge_value(&mut self, e: &mut IREdge, key: &ast::Key, _edge_idx: usize) {
        if key.edge_key.is_some() {
            // Edge key: set a field on the edge's map
            if e.map.is_none() {
                e.map = Some(Map::new());
            }
            if let Some(ref ekp) = key.edge_key {
                self.compile_field(e.map.as_mut().unwrap(), ekp, key);
            }
        } else {
            // Direct value on edge
            if let Some(ref primary) = key.primary {
                match primary {
                    ast::ScalarBox::Null(_) => {}
                    ast::ScalarBox::Suspension(s) => {
                        e.suspended = Some(s.value);
                    }
                    _ => {
                        e.primary = Some(Scalar {
                            value: primary.clone(),
                        });
                    }
                }
            }
            if let Some(ref val) = key.value {
                match val {
                    ast::ValueBox::Array(_) => {
                        // "edges cannot be assigned arrays" -- just skip
                    }
                    ast::ValueBox::Map(m) => {
                        if e.map.is_none() {
                            e.map = Some(Map::new());
                        }
                        self.compile_map(e.map.as_mut().unwrap(), m);
                    }
                    ast::ValueBox::Null(_) => {}
                    ast::ValueBox::Suspension(s) => {
                        e.suspended = Some(s.value);
                    }
                    _ => {
                        if let Some(sb) = val.scalar_box() {
                            if let ast::ScalarBox::Suspension(s) = sb {
                                e.suspended = Some(s.value);
                            } else {
                                e.primary = Some(Scalar { value: sb });
                            }
                        }
                    }
                }
            }
        }
    }

    fn compile_array(&mut self, dst: &mut Array, ast_arr: &ast::Array) {
        for node in &ast_arr.nodes {
            match node {
                ast::ArrayNode::Array(a) => {
                    let mut inner = Array { values: Vec::new() };
                    self.compile_array(&mut inner, a);
                    dst.values.push(Value::Array(inner));
                }
                ast::ArrayNode::Map(m) => {
                    let mut inner = Map::new();
                    self.compile_map(&mut inner, m);
                    dst.values.push(Value::Map(inner));
                }
                ast::ArrayNode::Null(n) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::Null(n.clone()),
                    }));
                }
                ast::ArrayNode::Boolean(b) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::Boolean(b.clone()),
                    }));
                }
                ast::ArrayNode::Number(n) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::Number(n.clone()),
                    }));
                }
                ast::ArrayNode::UnquotedString(s) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::UnquotedString(s.clone()),
                    }));
                }
                ast::ArrayNode::DoubleQuotedString(s) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::DoubleQuotedString(s.clone()),
                    }));
                }
                ast::ArrayNode::SingleQuotedString(s) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::SingleQuotedString(s.clone()),
                    }));
                }
                ast::ArrayNode::BlockString(s) => {
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::BlockString(s.clone()),
                    }));
                }
                ast::ArrayNode::Substitution(subst) => {
                    // Preserve the substitution as a placeholder scalar so the
                    // later compile_substitutions pass can expand/resolve it
                    // against the vars stack. Mirrors Go d2ir.compileArray
                    // which wraps substitutions in a Scalar/UnquotedString.
                    dst.values.push(Value::Scalar(Scalar {
                        value: ast::ScalarBox::UnquotedString(ast::UnquotedString {
                            range: subst.range.clone(),
                            value: vec![ast::InterpolationBox {
                                string: None,
                                string_raw: None,
                                substitution: Some(subst.clone()),
                            }],
                            pattern: None,
                        }),
                    }));
                }
                _ => {} // Comments, imports - skip
            }
        }
    }

    // ----- Deferred glob edge application -----

    /// Re-apply every recorded glob edge pattern against the final map.
    /// Mirrors Go's `lazyGlobBeingApplied` loop: after the first pass,
    /// re-walk each `(* -> *)[*].…` / `(** -> **)[*].…` declaration and
    /// apply its value to every matching edge. Using a raw scope pointer
    /// is safe here because the root `Map` lives on the caller's stack
    /// for the duration of `compile()` and we never add new fields during
    /// this pass (only mutate existing edges' maps/references).
    fn apply_pending_globs(&mut self) {
        // Drain so each pending is applied at most once per pass. Globs
        // themselves do not register new globs in this code path.
        let pendings = std::mem::take(&mut self.pending_globs);
        for pg in pendings {
            let scope_map: &mut Map = unsafe { &mut *pg.scope_map };
            let ast_edge = match pg.key.edges.get(pg.edge_idx).cloned() {
                Some(e) => e,
                None => continue,
            };
            let src_path: Vec<String> = ast_edge
                .src
                .as_ref()
                .map(|kp| {
                    kp.path
                        .iter()
                        .map(|sb| sb.scalar_string().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let dst_path: Vec<String> = ast_edge
                .dst
                .as_ref()
                .map(|kp| {
                    kp.path
                        .iter()
                        .map(|sb| sb.scalar_string().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let src_arrow = ast_edge.src_arrow == "<";
            let dst_arrow = ast_edge.dst_arrow == ">";
            let edge_index = pg
                .key
                .edge_index
                .as_ref()
                .and_then(|ei| ei.int.map(|v| v as usize));
            let eid = EdgeID {
                src_path: src_path.clone(),
                dst_path: dst_path.clone(),
                src_arrow,
                dst_arrow,
                index: edge_index,
                glob: true,
            };
            let has_double_glob =
                src_path.iter().any(|s| s == "**") || dst_path.iter().any(|s| s == "**");
            let mut locs: Vec<(*mut Map, usize)> = Vec::new();
            if has_double_glob {
                collect_matching_edges_recursive(scope_map, &eid, &mut locs);
            } else {
                let p = scope_map as *mut Map;
                for (idx, e) in scope_map.edges.iter().enumerate() {
                    if e.id.matches(&eid) {
                        locs.push((p, idx));
                    }
                }
            }
            for (map_ptr, eidx) in &locs {
                let m = unsafe { &mut **map_ptr };
                let e = &mut m.edges[*eidx];
                e.references.push(EdgeReference {
                    context: RefContext {
                        key: pg.key.clone(),
                        edge_ast: Some(ast_edge.clone()),
                        scope_map_idx: None,
                    },
                });
                self.apply_edge_value(e, &pg.key, pg.edge_idx);
            }
        }
    }

    // ----- Variable substitution (simplified) -----

    fn compile_substitutions(&mut self, m: &mut Map, vars_stack: &[&Map]) {
        // Collect vars from this scope
        let vars_map: Option<Map> = {
            let vars_field = m
                .fields
                .iter()
                .find(|f| f.name == "vars" && f.name_is_unquoted);
            vars_field.and_then(|f| f.map().cloned())
        };

        let mut new_stack: Vec<&Map> = Vec::new();
        // We need to handle lifetime carefully; store vars_map and build stack
        if let Some(ref vm) = vars_map {
            new_stack.push(vm);
        }
        new_stack.extend_from_slice(vars_stack);

        let stack = if vars_map.is_some() {
            &new_stack[..]
        } else {
            vars_stack
        };

        // Process fields. Spread substitutions can remove placeholder fields,
        // so this loop has to be index-based rather than `for field in ...`.
        let mut i = 0;
        while i < m.fields.len() {
            if self.expand_field_spread_substitution(stack, m, i) {
                continue;
            }

            if m.fields[i].primary.is_some() {
                self.resolve_substitutions(stack, &mut m.fields[i]);
            }
            // Recurse into composite. Arrays also need substitution processing
            // (e.g. `constraint: [PK; ...${base-constraints}]`).
            let mut comp = m.fields[i].composite.take();
            match comp.as_mut() {
                Some(Composite::Map(inner)) => {
                    self.compile_substitutions(inner, stack);
                }
                Some(Composite::Array(inner)) => {
                    self.compile_substitutions_array(inner, stack);
                }
                None => {}
            }
            m.fields[i].composite = comp;
            i += 1;
        }

        // Process edges
        for i in 0..m.edges.len() {
            if m.edges[i].primary.is_some() {
                self.resolve_edge_substitutions(stack, &mut m.edges[i]);
            }
            let has_map = m.edges[i].map.is_some();
            if has_map {
                let mut emap = m.edges[i].map.take();
                if let Some(ref mut inner) = emap {
                    self.compile_substitutions(inner, stack);
                }
                m.edges[i].map = emap;
            }
        }
    }

    /// Resolve substitutions appearing inside an IR array. Mirrors the
    /// `*Scalar` (array value) branch of Go `d2ir.resolveSubstitutions`:
    ///
    ///  * `...${var}` spreads another array in-place.
    ///  * `${var}` or `"pre ${var} post"` is replaced by the var's primary
    ///    value (or value of the wrapping string).
    fn compile_substitutions_array(&mut self, arr: &mut Array, vars_stack: &[&Map]) {
        let mut i = 0;
        while i < arr.values.len() {
            let replacement = match &arr.values[i] {
                Value::Scalar(scalar) => self.resolve_array_scalar_substitution(vars_stack, scalar),
                _ => None,
            };
            match replacement {
                Some(ArraySubResult::Spread(values)) => {
                    // Replace arr.values[i] with the spread values.
                    let tail = arr.values.split_off(i + 1);
                    arr.values.pop();
                    let inserted = values.len();
                    arr.values.extend(values);
                    arr.values.extend(tail);
                    i += inserted;
                }
                Some(ArraySubResult::Scalar(s)) => {
                    arr.values[i] = Value::Scalar(s);
                    i += 1;
                }
                None => {
                    // Recurse into nested composites.
                    match &mut arr.values[i] {
                        Value::Map(m) => self.compile_substitutions(m, vars_stack),
                        Value::Array(a) => self.compile_substitutions_array(a, vars_stack),
                        Value::Scalar(_) => {}
                    }
                    i += 1;
                }
            }
        }
    }

    /// Inspect an array-element Scalar for substitutions. Returns
    /// `Some(Spread)` when the element is solely a spread `...${var}`
    /// (caller must splice the resolved values in), `Some(Scalar)` when
    /// an interpolated scalar substitution resolved to a new scalar
    /// value, or `None` when no substitution is present.
    fn resolve_array_scalar_substitution(
        &mut self,
        vars_stack: &[&Map],
        scalar: &Scalar,
    ) -> Option<ArraySubResult> {
        let us = match &scalar.value {
            ast::ScalarBox::UnquotedString(us) => us,
            _ => return None,
        };
        // Sole spread: `...${var}`.
        if us.value.len() == 1
            && let Some(sub) = us.value[0].substitution.as_ref()
            && sub.spread
        {
            let resolved = self.resolve_substitution_field(vars_stack, sub)?;
            let Some(comp) = resolved.composite.as_ref() else {
                self.errorf(&sub.range, "cannot spread non-composite".to_string());
                return Some(ArraySubResult::Spread(Vec::new()));
            };
            let Composite::Array(resolved_arr) = comp else {
                self.errorf(&sub.range, "cannot spread non-array into array".to_string());
                return Some(ArraySubResult::Spread(Vec::new()));
            };
            return Some(ArraySubResult::Spread(resolved_arr.values.clone()));
        }

        // Any interpolation boxes to resolve?
        let has_any_sub = us.value.iter().any(|b| b.substitution.is_some());
        if !has_any_sub {
            return None;
        }

        // Exactly one box that is a substitution with a composite value →
        // replace the scalar with that composite's primary string. Only
        // scalar-primary replacements are supported here (matches how
        // constraint/class arrays consume scalar values downstream).
        if us.value.len() == 1
            && let Some(sub) = us.value[0].substitution.as_ref()
        {
            let resolved = self.resolve_substitution_field(vars_stack, sub)?;
            if let Some(primary) = resolved.primary.as_ref() {
                return Some(ArraySubResult::Scalar(Scalar {
                    value: primary.value.clone(),
                }));
            }
            return None;
        }

        // Mixed interpolation: build a flat string.
        let mut flat = String::new();
        for box_ in &us.value {
            if let Some(ref s) = box_.string {
                flat.push_str(s);
            } else if let Some(ref sub) = box_.substitution {
                let resolved_val = self.resolve_substitution(vars_stack, sub)?;
                flat.push_str(&resolved_val);
            }
        }
        Some(ArraySubResult::Scalar(Scalar {
            value: ast::ScalarBox::UnquotedString(ast::flat_unquoted_string(&flat)),
        }))
    }

    fn expand_field_spread_substitution(
        &mut self,
        vars_stack: &[&Map],
        m: &mut Map,
        field_idx: usize,
    ) -> bool {
        let Some(primary) = m.fields[field_idx].primary.as_ref() else {
            return false;
        };
        let Some(sub) = extract_single_substitution(&primary.value) else {
            return false;
        };
        if !sub.spread || !m.fields[field_idx].name.is_empty() {
            return false;
        }

        let resolved = match self.resolve_substitution_field(vars_stack, sub) {
            Some(field) => field,
            None => return false,
        };

        let Some(resolved_map) = resolved.map() else {
            self.errorf(&sub.range, "cannot spread non-composite".to_string());
            return false;
        };

        let resolved_map = resolved_map.clone();
        m.fields.remove(field_idx);
        expand_substitution(m, &resolved_map, field_idx);
        true
    }

    fn resolve_substitutions(&mut self, vars_stack: &[&Map], f: &mut Field) {
        let primary = match f.primary.as_ref() {
            Some(p) => p,
            None => return,
        };

        // Check if the primary value contains substitutions
        if let ast::ScalarBox::UnquotedString(ref us) = primary.value {
            for ibox in &us.value {
                if let Some(ref sub) = ibox.substitution {
                    let resolved = self.resolve_substitution(vars_stack, sub);
                    if let Some(resolved_val) = resolved {
                        f.primary = Some(Scalar {
                            value: ast::ScalarBox::UnquotedString(ast::flat_unquoted_string(
                                &resolved_val,
                            )),
                        });
                    }
                    return;
                }
            }
        }

        if let ast::ScalarBox::DoubleQuotedString(ref dqs) = primary.value {
            for ibox in &dqs.value {
                if let Some(ref sub) = ibox.substitution {
                    let resolved = self.resolve_substitution(vars_stack, sub);
                    if let Some(resolved_val) = resolved {
                        f.primary = Some(Scalar {
                            value: ast::ScalarBox::DoubleQuotedString(
                                ast::flat_double_quoted_string(&resolved_val),
                            ),
                        });
                    }
                    return;
                }
            }
        }

        // BlockString (|md ...|) supports variable substitution in non-code
        // text. Mirror Go `textmeasure.ReplaceSubstitutionsMarkdown`.
        if let ast::ScalarBox::BlockString(ref bs) = primary.value {
            let mut variables = std::collections::BTreeMap::new();
            for vars_map in vars_stack {
                collect_variables_flat(vars_map, String::new(), &mut variables);
            }
            if !variables.is_empty() {
                let new_val = replace_substitutions_markdown(&bs.value, &variables);
                if new_val != bs.value {
                    let mut new_bs = bs.clone();
                    new_bs.value = new_val;
                    f.primary = Some(Scalar {
                        value: ast::ScalarBox::BlockString(new_bs),
                    });
                }
            }
        }
    }

    fn resolve_edge_substitutions(&mut self, vars_stack: &[&Map], e: &mut IREdge) {
        let primary = match e.primary.as_ref() {
            Some(p) => p,
            None => return,
        };

        if let ast::ScalarBox::UnquotedString(ref us) = primary.value {
            for ibox in &us.value {
                if let Some(ref sub) = ibox.substitution {
                    let resolved = self.resolve_substitution(vars_stack, sub);
                    if let Some(resolved_val) = resolved {
                        e.primary = Some(Scalar {
                            value: ast::ScalarBox::UnquotedString(ast::flat_unquoted_string(
                                &resolved_val,
                            )),
                        });
                    }
                    return;
                }
            }
        }
    }

    fn resolve_substitution(
        &mut self,
        vars_stack: &[&Map],
        sub: &ast::Substitution,
    ) -> Option<String> {
        let path: Vec<String> = sub
            .path
            .iter()
            .map(|sb| sb.scalar_string().to_string())
            .collect();

        for vars in vars_stack {
            if let Some(field) = self.lookup_var_field(vars, &path)
                && let Some(val) = field.primary_string()
            {
                return Some(val);
            }
        }
        self.errorf(
            &sub.range,
            format!("could not resolve variable \"{}\"", path.join(".")),
        );
        None
    }

    fn resolve_substitution_field(
        &mut self,
        vars_stack: &[&Map],
        sub: &ast::Substitution,
    ) -> Option<Field> {
        let path: Vec<String> = sub
            .path
            .iter()
            .map(|sb| sb.scalar_string().to_string())
            .collect();

        for vars in vars_stack {
            if let Some(field) = self.lookup_var_field(vars, &path) {
                return Some(field.clone());
            }
        }

        self.errorf(
            &sub.range,
            format!("could not resolve variable \"{}\"", path.join(".")),
        );
        None
    }

    fn lookup_var_field<'a>(&self, vars: &'a Map, path: &[String]) -> Option<&'a Field> {
        lookup_var_field(vars, path)
    }
}

fn lookup_var_field<'a>(vars: &'a Map, path: &[String]) -> Option<&'a Field> {
    if path.is_empty() {
        return None;
    }
    let f = vars.get_field(&path[0])?;
    if path.len() == 1 {
        return Some(f);
    }
    let inner = f.map()?;
    lookup_var_field(inner, &path[1..])
}

/// Collect all scalar variables from a `vars` map into a flat
/// `name.path` → value map. Matches Go `d2ir.collectVariables`.
fn collect_variables_flat(
    vars: &Map,
    prefix: String,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    for f in &vars.fields {
        if let Some(p) = f.primary.as_ref() {
            let key = if prefix.is_empty() {
                f.name.clone()
            } else {
                format!("{}.{}", prefix, f.name)
            };
            out.insert(key, p.scalar_string().to_string());
        } else if let Some(m) = f.map() {
            let new_prefix = if prefix.is_empty() {
                f.name.clone()
            } else {
                format!("{}.{}", prefix, f.name)
            };
            collect_variables_flat(m, new_prefix, out);
            // Go also adds nested vars with short names directly at the top
            // level via a separate `collectVariables(f.Map(), variables)`
            // recursion — emulate by recursing without prefix.
            collect_variables_flat(m, prefix.clone(), out);
        }
    }
}

/// Port of Go `textmeasure.ReplaceSubstitutionsMarkdown`: replace
/// `${key}` occurrences with variable values, but skip code spans
/// (backticks) and fenced code blocks (``` lines).
fn replace_substitutions_markdown(
    md: &str,
    variables: &std::collections::BTreeMap<String, String>,
) -> String {
    // Process line by line to detect fenced code blocks, and character by
    // character within each line to skip code spans.
    let mut out = String::with_capacity(md.len());
    let mut in_fenced = false;
    for (i, line) in md.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_start();
        // Detect fenced code block boundary: line starting with ``` (or more)
        if trimmed.starts_with("```") {
            in_fenced = !in_fenced;
            out.push_str(line);
            continue;
        }
        if in_fenced {
            out.push_str(line);
            continue;
        }
        // Replace `${var}` in this line, skipping inline code spans marked
        // by backticks.
        let _ = i;
        replace_substitutions_line(line, variables, &mut out);
    }
    out
}

fn replace_substitutions_line(
    line: &str,
    variables: &std::collections::BTreeMap<String, String>,
    out: &mut String,
) {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            // Inline code span: find the closing backtick
            out.push('`');
            i += 1;
            while i < bytes.len() && bytes[i] != b'`' {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                out.push('`');
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Find the matching '}'
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'}' {
                end += 1;
            }
            if end < bytes.len() {
                let key = &line[start..end];
                if let Some(val) = variables.get(key) {
                    out.push_str(val);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
}

/// Result of resolving a substitution inside an array element.
enum ArraySubResult {
    /// The element was a solo `...${var}` spread; splice these values in.
    Spread(Vec<Value>),
    /// The element was a scalar-valued substitution; replace with this.
    Scalar(Scalar),
}

fn extract_single_substitution(value: &ast::ScalarBox) -> Option<&ast::Substitution> {
    match value {
        ast::ScalarBox::UnquotedString(us) if us.value.len() == 1 => {
            us.value[0].substitution.as_ref()
        }
        ast::ScalarBox::DoubleQuotedString(dqs) if dqs.value.len() == 1 => {
            dqs.value[0].substitution.as_ref()
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_str(input: &str) -> Result<Map, CompileError> {
        let (ast, err) = crate::parser::parse("test.d2", input);
        if let Some(e) = err {
            return Err(CompileError { errors: e.errors });
        }
        compile(&ast)
    }

    #[test]
    fn test_single_field() {
        let m = compile_str("x").unwrap();
        assert_eq!(m.fields.len(), 1);
        assert_eq!(m.fields[0].name, "x");
    }

    #[test]
    fn test_field_with_label() {
        let m = compile_str("x: hello").unwrap();
        assert_eq!(m.fields.len(), 1);
        assert_eq!(m.fields[0].name, "x");
        assert_eq!(m.fields[0].primary_string(), Some("hello".to_string()));
    }

    #[test]
    fn test_nested_field() {
        let m = compile_str("a.b.c: val").unwrap();
        assert_eq!(m.fields.len(), 1);
        assert_eq!(m.fields[0].name, "a");
        let b = m.fields[0].map().unwrap().get_field("b").unwrap();
        let c = b.map().unwrap().get_field("c").unwrap();
        assert_eq!(c.primary_string(), Some("val".to_string()));
    }

    #[test]
    fn test_simple_edge() {
        let m = compile_str("a -> b").unwrap();
        assert_eq!(m.edges.len(), 1);
        assert_eq!(m.edges[0].id.src_path, vec!["a"]);
        assert_eq!(m.edges[0].id.dst_path, vec!["b"]);
        assert!(!m.edges[0].id.src_arrow);
        assert!(m.edges[0].id.dst_arrow);
        // Both src and dst should be created as fields
        assert!(m.get_field("a").is_some());
        assert!(m.get_field("b").is_some());
    }

    #[test]
    fn test_edge_with_label() {
        let m = compile_str("a -> b: hello").unwrap();
        assert_eq!(m.edges.len(), 1);
        assert_eq!(
            m.edges[0].primary.as_ref().unwrap().scalar_string(),
            "hello"
        );
    }

    #[test]
    fn test_bidirectional_edge() {
        let m = compile_str("a <-> b").unwrap();
        assert_eq!(m.edges.len(), 1);
        assert!(m.edges[0].id.src_arrow);
        assert!(m.edges[0].id.dst_arrow);
    }

    #[test]
    fn test_multiple_edges() {
        let m = compile_str("a -> b\na -> b").unwrap();
        assert_eq!(m.edges.len(), 2);
        assert_eq!(m.edges[0].id.index, Some(0));
        assert_eq!(m.edges[1].id.index, Some(1));
    }

    #[test]
    fn test_edge_chain() {
        let m = compile_str("a -> b -> c").unwrap();
        assert_eq!(m.edges.len(), 2);
        assert_eq!(m.edges[0].id.src_path, vec!["a"]);
        assert_eq!(m.edges[0].id.dst_path, vec!["b"]);
        assert_eq!(m.edges[1].id.src_path, vec!["b"]);
        assert_eq!(m.edges[1].id.dst_path, vec!["c"]);
    }

    #[test]
    fn test_field_with_map() {
        let m = compile_str("x: {\n  shape: circle\n}").unwrap();
        assert_eq!(m.fields.len(), 1);
        let xmap = m.fields[0].map().unwrap();
        let shape = xmap.get_field("shape").unwrap();
        assert_eq!(shape.primary_string(), Some("circle".to_string()));
    }

    #[test]
    fn test_style_field() {
        let m = compile_str("x: {\n  style.opacity: 0.4\n}").unwrap();
        let xmap = m.fields[0].map().unwrap();
        let style = xmap.get_field("style").unwrap();
        let opacity = style.map().unwrap().get_field("opacity").unwrap();
        assert_eq!(opacity.primary_string(), Some("0.4".to_string()));
    }

    #[test]
    fn test_edge_with_map() {
        let m = compile_str("a -> b: {\n  style.stroke: red\n}").unwrap();
        assert_eq!(m.edges.len(), 1);
        let emap = m.edges[0].map.as_ref().unwrap();
        let style = emap.get_field("style").unwrap();
        let stroke = style.map().unwrap().get_field("stroke").unwrap();
        assert_eq!(stroke.primary_string(), Some("red".to_string()));
    }

    #[test]
    fn test_vars_substitution() {
        let m = compile_str("vars: {\n  color: red\n}\nx: ${color}").unwrap();
        let x = m.get_field("x").unwrap();
        assert_eq!(x.primary_string(), Some("red".to_string()));
    }

    #[test]
    fn test_vars_spread_in_place() {
        let m = compile_str(
            "vars: {\n  person-shape: {\n    grid-columns: 1\n    grid-rows: 2\n    grid-gap: 0\n    head\n    body\n  }\n}\n\ndora: {\n  ...${person-shape}\n  body\n}\n",
        )
        .unwrap();

        let dora = m.get_field("dora").unwrap().map().unwrap();
        assert_eq!(dora.fields[0].name, "grid-columns");
        assert!(dora.get_field("body").is_some());
    }

    #[test]
    fn test_vars_spread_hyphenated_name() {
        let m = compile_str(
            "vars: {\n  dog1: Frido {\n    shape: circle\n  }\n  my-house: {\n    label: Home\n  }\n}\n\nok: {\n  ...${my-house}\n  dog1: {\n    ...${dog1}\n  }\n  dog1 -> dog3\n}\n",
        )
        .unwrap();

        let ok = m.get_field("ok").unwrap().map().unwrap();
        let label = ok.get_field("label").unwrap();
        assert_eq!(label.primary_string(), Some("Home".to_string()));
        let dog1 = ok.get_field("dog1").unwrap().map().unwrap();
        let shape = dog1.get_field("shape").unwrap();
        assert_eq!(shape.primary_string(), Some("circle".to_string()));
    }

    #[test]
    fn test_field_override() {
        let m = compile_str("x: hello\nx: world").unwrap();
        assert_eq!(m.fields.len(), 1);
        assert_eq!(m.fields[0].primary_string(), Some("world".to_string()));
    }

    #[test]
    fn test_reserved_keyword_shape() {
        let m = compile_str("x: { shape: circle }").unwrap();
        let xmap = m.fields[0].map().unwrap();
        assert!(xmap.get_field("shape").is_some());
    }

    #[test]
    fn test_overlay_map() {
        let mut base = Map::new();
        base.fields.push(Field {
            name: "x".to_string(),
            name_is_unquoted: true,
            primary: Some(Scalar {
                value: ast::ScalarBox::UnquotedString(ast::flat_unquoted_string("old")),
            }),
            composite: None,
            references: Vec::new(),
            suspended: None,
        });

        let overlay = Map {
            fields: vec![Field {
                name: "x".to_string(),
                name_is_unquoted: true,
                primary: Some(Scalar {
                    value: ast::ScalarBox::UnquotedString(ast::flat_unquoted_string("new")),
                }),
                composite: None,
                references: Vec::new(),
                suspended: None,
            }],
            edges: Vec::new(),
            filters: Vec::new(),
        };

        overlay_map(&mut base, &overlay);
        assert_eq!(base.fields[0].primary_string(), Some("new".to_string()));
    }

    #[test]
    fn test_common_edge_prefix() {
        let m = compile_str("a.b -> a.c").unwrap();
        // The edge should be inside 'a' due to common prefix resolution
        let a = m.get_field("a").unwrap();
        let amap = a.map().unwrap();
        assert_eq!(amap.edges.len(), 1);
        assert_eq!(amap.edges[0].id.src_path, vec!["b"]);
        assert_eq!(amap.edges[0].id.dst_path, vec!["c"]);
    }
}
