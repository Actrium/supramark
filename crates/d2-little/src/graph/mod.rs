//! d2-graph: core graph types for d2 diagram compilation and layout.
//!
//! These types bridge the d2 AST/IR with layout engines and exporters.
//! Ported from Go `d2graph/d2graph.go`.

use crate::ast;
use crate::geo::{Box2D, Point, Segment, Spacing};
use crate::shape::ShapeOps;

pub mod go_sort;

// ---------------------------------------------------------------------------
// Key ID formatting — mirrors Go's d2format.Format(RawString(s, inKey=true))
// ---------------------------------------------------------------------------

/// Characters that force quoting of a key segment.  Matches Go
/// `d2ast.UnquotedKeySpecials`.
const UNQUOTED_KEY_SPECIALS: &str = "#;\n\\{}[]'\"|:.-<>*&()@&";

/// Format a key segment for use as an object ID, matching Go's
/// `d2format.Format(RawString(name, true))`.
///
/// If `name` contains characters from `UNQUOTED_KEY_SPECIALS`, the
/// result is a double-quoted (or single-quoted) string.  Otherwise
/// the name is returned unchanged.
///
/// Inside double-quoted output, `"` and `\` are backslash-escaped
/// and literal newlines become `\n`.
pub fn format_key_segment(name: &str) -> String {
    if name.is_empty() {
        return "\"\"".to_owned();
    }
    let needs_quoting = name.chars().any(|ch| {
        if ch == '-' {
            // A lone '-' does NOT require quoting; only '--' does.
            // However, the simple char-level check here matches Go
            // RawString which checks for '-' only when next char is
            // also '-'. For correctness we just check the set (Go
            // itself iterates runes and only skips single '-').
            // We'll handle the dash special-case below.
            return false;
        }
        UNQUOTED_KEY_SPECIALS.contains(ch)
    }) || name.contains("--");

    if !needs_quoting {
        // Check surrounding whitespace
        let has_surrounding_ws =
            name.starts_with(char::is_whitespace) || name.ends_with(char::is_whitespace);
        if !has_surrounding_ws {
            return name.to_owned();
        }
    }

    // Prefer double quotes unless the string contains '"'
    if !name.contains('"') {
        let mut out = String::with_capacity(name.len() + 2);
        out.push('"');
        for ch in name.chars() {
            match ch {
                '\\' => {
                    out.push('\\');
                    out.push('\\');
                }
                '\n' => {
                    out.push('\\');
                    out.push('n');
                }
                _ => out.push(ch),
            }
        }
        out.push('"');
        return out;
    }

    // Fallback to single quotes
    if !name.contains('\n') {
        let mut out = String::with_capacity(name.len() + 2);
        out.push('\'');
        for ch in name.chars() {
            if ch == '\'' {
                out.push('\''); // escape ' by doubling
            }
            out.push(ch);
        }
        out.push('\'');
        return out;
    }

    // Both " and \n — use double quotes and escape "
    let mut out = String::with_capacity(name.len() + 4);
    out.push('"');
    for ch in name.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => {
                out.push('\\');
                out.push('n');
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Strip surrounding quotes from a formatted key segment, returning
/// the unquoted (IDVal) form.  If the string is not quoted, it is
/// returned unchanged.
pub fn unformat_key_segment(formatted: &str) -> &str {
    if formatted.len() >= 2
        && ((formatted.starts_with('"') && formatted.ends_with('"'))
            || (formatted.starts_with('\'') && formatted.ends_with('\'')))
    {
        return &formatted[1..formatted.len() - 1];
    }
    formatted
}

/// Compare two AST source ranges by start position. Mirrors Go's
/// `Range.Before` ordering used in `SortObjectsByAST`.
pub fn range_cmp(a: &ast::Range, b: &ast::Range) -> std::cmp::Ordering {
    a.start
        .byte
        .cmp(&b.start.byte)
        .then(a.end.byte.cmp(&b.end.byte))
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum segment length for edge routing. Shorter segments near arrowheads
/// get extended to avoid rendering artifacts.
pub const MIN_SEGMENT_LEN: f64 = 10.0;

/// Default padding around container contents.
pub const DEFAULT_PADDING: f64 = 30.0;

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Direction {
    pub value: String,
}

// ---------------------------------------------------------------------------
// Dimensions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct Dimensions {
    pub width: i32,
    pub height: i32,
}

// ---------------------------------------------------------------------------
// Style (scalar wrapper)
// ---------------------------------------------------------------------------

/// A single style value from the DSL.
#[derive(Debug, Clone, Default)]
pub struct ScalarValue {
    pub value: String,
}

/// Style properties that can be set on objects and edges.
#[derive(Debug, Clone, Default)]
pub struct Style {
    pub opacity: Option<ScalarValue>,
    pub stroke: Option<ScalarValue>,
    pub fill: Option<ScalarValue>,
    pub fill_pattern: Option<ScalarValue>,
    pub stroke_dash: Option<ScalarValue>,
    pub stroke_width: Option<ScalarValue>,
    pub shadow: Option<ScalarValue>,
    pub three_dee: Option<ScalarValue>,
    pub multiple: Option<ScalarValue>,
    pub border_radius: Option<ScalarValue>,
    pub font_color: Option<ScalarValue>,
    pub font_size: Option<ScalarValue>,
    pub italic: Option<ScalarValue>,
    pub bold: Option<ScalarValue>,
    pub underline: Option<ScalarValue>,
    pub font: Option<ScalarValue>,
    pub double_border: Option<ScalarValue>,
    pub animated: Option<ScalarValue>,
    pub filled: Option<ScalarValue>,
    pub text_transform: Option<ScalarValue>,
}

impl Style {
    /// Initialize a style field to `Some(ScalarValue::default())` so `apply` can set it.
    pub fn init_field(&mut self, key: &str) {
        match key {
            "opacity" if self.opacity.is_none() => {
                self.opacity = Some(ScalarValue::default());
            }
            "stroke" if self.stroke.is_none() => {
                self.stroke = Some(ScalarValue::default());
            }
            "fill" if self.fill.is_none() => {
                self.fill = Some(ScalarValue::default());
            }
            "fill-pattern" if self.fill_pattern.is_none() => {
                self.fill_pattern = Some(ScalarValue::default());
            }
            "stroke-width" if self.stroke_width.is_none() => {
                self.stroke_width = Some(ScalarValue::default());
            }
            "stroke-dash" if self.stroke_dash.is_none() => {
                self.stroke_dash = Some(ScalarValue::default());
            }
            "border-radius" if self.border_radius.is_none() => {
                self.border_radius = Some(ScalarValue::default());
            }
            "shadow" if self.shadow.is_none() => {
                self.shadow = Some(ScalarValue::default());
            }
            "3d" if self.three_dee.is_none() => {
                self.three_dee = Some(ScalarValue::default());
            }
            "multiple" if self.multiple.is_none() => {
                self.multiple = Some(ScalarValue::default());
            }
            "font" if self.font.is_none() => {
                self.font = Some(ScalarValue::default());
            }
            "font-size" if self.font_size.is_none() => {
                self.font_size = Some(ScalarValue::default());
            }
            "font-color" if self.font_color.is_none() => {
                self.font_color = Some(ScalarValue::default());
            }
            "animated" if self.animated.is_none() => {
                self.animated = Some(ScalarValue::default());
            }
            "bold" if self.bold.is_none() => {
                self.bold = Some(ScalarValue::default());
            }
            "italic" if self.italic.is_none() => {
                self.italic = Some(ScalarValue::default());
            }
            "underline" if self.underline.is_none() => {
                self.underline = Some(ScalarValue::default());
            }
            "filled" if self.filled.is_none() => {
                self.filled = Some(ScalarValue::default());
            }
            "double-border" if self.double_border.is_none() => {
                self.double_border = Some(ScalarValue::default());
            }
            "text-transform" if self.text_transform.is_none() => {
                self.text_transform = Some(ScalarValue::default());
            }
            _ => {}
        }
    }

    /// Apply a style key-value pair, validating the value.
    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "opacity" => {
                if let Some(s) = self.opacity.as_mut() {
                    let f: f64 = value.parse().map_err(|_| {
                        "expected \"opacity\" to be a number between 0.0 and 1.0".to_string()
                    })?;
                    if !(0.0..=1.0).contains(&f) {
                        return Err(
                            "expected \"opacity\" to be a number between 0.0 and 1.0".to_string()
                        );
                    }
                    s.value = value.to_string();
                }
            }
            "stroke" => {
                if let Some(s) = self.stroke.as_mut() {
                    s.value = value.to_string();
                }
            }
            "fill" => {
                if let Some(s) = self.fill.as_mut() {
                    s.value = value.to_string();
                }
            }
            "fill-pattern" => {
                if let Some(s) = self.fill_pattern.as_mut() {
                    let patterns = ["none", "dots", "lines", "grain", "paper"];
                    if !patterns.contains(&value.to_lowercase().as_str()) {
                        return Err(format!(
                            "expected \"fill-pattern\" to be one of: {}",
                            patterns.join(", ")
                        ));
                    }
                    s.value = value.to_string();
                }
            }
            "stroke-width" => {
                if let Some(s) = self.stroke_width.as_mut() {
                    let v: i32 = value.parse().map_err(|_| {
                        "expected \"stroke-width\" to be a number between 0 and 15".to_string()
                    })?;
                    if !(0..=15).contains(&v) {
                        return Err(
                            "expected \"stroke-width\" to be a number between 0 and 15".to_string()
                        );
                    }
                    s.value = value.to_string();
                }
            }
            "stroke-dash" => {
                if let Some(s) = self.stroke_dash.as_mut() {
                    let v: i32 = value.parse().map_err(|_| {
                        "expected \"stroke-dash\" to be a number between 0 and 10".to_string()
                    })?;
                    if !(0..=10).contains(&v) {
                        return Err(
                            "expected \"stroke-dash\" to be a number between 0 and 10".to_string()
                        );
                    }
                    s.value = value.to_string();
                }
            }
            "border-radius" => {
                if let Some(s) = self.border_radius.as_mut() {
                    let v: i32 = value.parse().map_err(|_| {
                        "expected \"border-radius\" to be a number >= 0".to_string()
                    })?;
                    if v < 0 {
                        return Err("expected \"border-radius\" to be a number >= 0".to_string());
                    }
                    s.value = value.to_string();
                }
            }
            "shadow" | "3d" | "multiple" | "animated" | "bold" | "italic" | "underline"
            | "filled" | "double-border" => {
                let target = match key {
                    "shadow" => self.shadow.as_mut(),
                    "3d" => self.three_dee.as_mut(),
                    "multiple" => self.multiple.as_mut(),
                    "animated" => self.animated.as_mut(),
                    "bold" => self.bold.as_mut(),
                    "italic" => self.italic.as_mut(),
                    "underline" => self.underline.as_mut(),
                    "filled" => self.filled.as_mut(),
                    "double-border" => self.double_border.as_mut(),
                    _ => None,
                };
                if let Some(s) = target {
                    value
                        .parse::<bool>()
                        .map_err(|_| format!("expected \"{}\" to be true or false", key))?;
                    s.value = value.to_string();
                }
            }
            "font" => {
                if let Some(s) = self.font.as_mut() {
                    s.value = value.to_string();
                }
            }
            "font-size" => {
                if let Some(s) = self.font_size.as_mut() {
                    let v: i32 = value.parse().map_err(|_| {
                        "expected \"font-size\" to be a number between 8 and 100".to_string()
                    })?;
                    if !(8..=100).contains(&v) {
                        return Err(
                            "expected \"font-size\" to be a number between 8 and 100".to_string()
                        );
                    }
                    s.value = value.to_string();
                }
            }
            "font-color" => {
                if let Some(s) = self.font_color.as_mut() {
                    s.value = value.to_string();
                }
            }
            "text-transform" => {
                if let Some(s) = self.text_transform.as_mut() {
                    let vals = ["none", "uppercase", "lowercase", "capitalize"];
                    if !vals.contains(&value) {
                        return Err(format!(
                            "expected \"text-transform\" to be one of: {}",
                            vals.join(", ")
                        ));
                    }
                    s.value = value.to_string();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Icon-specific style overrides.
#[derive(Debug, Clone, Default)]
pub struct IconStyle {
    pub border_radius: Option<ScalarValue>,
}

// ---------------------------------------------------------------------------
// Label
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Label {
    pub value: String,
    pub map_key: Option<()>, // simplified: just presence/absence
}

// ---------------------------------------------------------------------------
// MText (measured text associated with objects/edges)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct MText {
    pub text: String,
    pub font_size: i32,
    pub is_bold: bool,
    pub is_italic: bool,
    pub dimensions: Dimensions,
}

impl MText {
    pub fn get_color(&self, is_italic: bool) -> &str {
        if is_italic {
            crate::color::N2
        } else {
            crate::color::N1
        }
    }
}

// ---------------------------------------------------------------------------
// Arrowhead info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ArrowheadInfo {
    pub label: Label,
    pub label_dimensions: Dimensions,
    pub style: Style,
    pub shape: Option<String>,
    pub filled: Option<bool>,
}

impl ArrowheadInfo {
    /// Convert to a target arrowhead type.
    pub fn to_arrowhead(&self) -> crate::target::Arrowhead {
        let shape_str = self.shape.as_deref().unwrap_or("");
        crate::target::Arrowhead::to_arrowhead(shape_str, self.filled)
    }
}

// ---------------------------------------------------------------------------
// GlobFilter
// ---------------------------------------------------------------------------

/// `&attr: value` (or `!&attr: value`) filter declared inside a glob
/// template map. Used by the compiler to narrow which fields receive
/// the template's values when expanding `*` / `**`.
#[derive(Debug, Clone)]
pub struct GlobFilter {
    /// Dotted attribute path (e.g. `shape`, `class`, `style.fill`, `label`).
    pub attr: Vec<String>,
    /// Expected value (lowercase-compared).
    pub value: String,
    /// `true` when declared with `!&`.
    pub negate: bool,
}

// ---------------------------------------------------------------------------
// ObjId
// ---------------------------------------------------------------------------

/// Unique object identifier within a graph. Uses an index for efficient lookup.
pub type ObjId = usize;

// ---------------------------------------------------------------------------
// Object
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Object {
    pub id: String,
    /// Unescaped ID value — mirrors Go `Object.IDVal`. Formatted id retains
    /// source-level quoting/escaping (e.g. `"a\\yode"`); id_val holds the
    /// scalar string a user would see (e.g. `a\yode`). Required for
    /// child lookup parity with incoming IR field names which are raw.
    pub id_val: String,
    pub abs_id: String,

    pub label: Label,
    pub shape: ScalarValue,
    pub direction: Direction,
    pub language: String,

    pub top_left: Point,
    pub width: f64,
    pub height: f64,

    /// Bounding box, updated during layout.
    pub box_: Box2D,

    pub parent: Option<ObjId>,
    pub children: Vec<ObjId>,
    pub children_array: Vec<ObjId>,

    pub style: Style,
    pub icon_style: IconStyle,

    pub icon: Option<String>,
    pub icon_position: Option<String>,
    pub label_position: Option<String>,
    pub tooltip_position: Option<String>,
    pub label_dimensions: Dimensions,

    pub tooltip: Option<ScalarValue>,
    pub link: Option<ScalarValue>,

    pub class: Option<crate::target::Class>,
    pub sql_table: Option<crate::target::SQLTable>,
    pub content_aspect_ratio: Option<f64>,

    // Attributes from Go's Attributes struct
    pub width_attr: Option<ScalarValue>,
    pub height_attr: Option<ScalarValue>,
    pub top: Option<ScalarValue>,
    pub left: Option<ScalarValue>,
    pub near_key: Option<String>,
    pub constraint: Vec<String>,
    pub grid_rows: Option<ScalarValue>,
    pub grid_columns: Option<ScalarValue>,
    /// Source range of the `grid-rows` key, used to determine direction
    /// precedence when both `grid-rows` and `grid-columns` are set.
    pub grid_rows_range: Option<ast::Range>,
    /// Source range of the `grid-columns` key.
    pub grid_columns_range: Option<ast::Range>,
    pub grid_gap: Option<ScalarValue>,
    pub vertical_gap: Option<ScalarValue>,
    pub horizontal_gap: Option<ScalarValue>,

    pub z_index: i32,
    pub classes: Vec<String>,
    /// True when `classes` was assigned via the object's own `class:`
    /// declaration in the source (rather than inherited from a `*` /
    /// `**` glob template). Used during glob template merging to decide
    /// whether the template's class list (and class-derived styles) may
    /// override the target's existing values: if the target has its own
    /// explicit class list, glob-supplied class defaults are skipped so
    /// that own-class semantics win, mirroring Go d2ir's array-composite
    /// replacement behavior at IR time.
    pub classes_explicit: bool,

    /// `&attr: value` filters declared on this object's map. Used by the
    /// compiler when expanding glob (`*` / `**`) templates to restrict
    /// which targets receive the template's values.
    pub filters: Vec<GlobFilter>,

    /// Set to `true` by the sequence layout for note objects. Used by
    /// `get_fill` to return the correct note fill color (N7).
    pub is_sequence_diagram_note: bool,

    /// Set to `true` by the sequence layout for group objects. Used by
    /// `get_fill` to return the correct group fill color (N5), and by the
    /// exporter to set `stroke_width=0`, `blend=true`, `label_fill`.
    pub is_sequence_diagram_group: bool,

    /// AST references that named this object. Mirrors Go
    /// d2graph.Object.References. Used by `Graph::sort_objects_by_ast` to
    /// reorder objects to match the order they first appear in the source.
    pub references: Vec<Reference>,

    /// Suspension state propagated from IR fields and glob templates.
    /// `Some(true)` hides the object (and its subtree) from the final
    /// graph; `Some(false)` forces it visible (used to override a parent
    /// glob's suspend). `None` means untouched — treated as visible
    /// unless a glob flips it. Mirrors Go d2ir.Field.suspended.
    pub suspended: Option<bool>,
}

/// AST reference back-pointer for an object. Mirrors Go d2graph.Reference
/// for the fields the sorter actually consumes (Key + KeyPathIndex).
#[derive(Debug, Clone)]
pub struct Reference {
    /// The originating AST KeyPath (e.g. for `g.b`, both g's and b's
    /// References point to the same `g.b` KeyPath, distinguished by
    /// `key_path_index`).
    pub key: ast::KeyPath,
    /// Index into `key.path` for the segment this reference represents.
    pub key_path_index: usize,
    /// The object in whose map scope this reference was declared. Mirrors
    /// Go `Reference.ScopeObj`. Used by sequence diagram layout to determine
    /// which objects are contained by a group.
    pub scope_obj: Option<ObjId>,
    /// True when this reference was carried over from a variable
    /// (spread) substitution and therefore points at the variable's
    /// declaration rather than the substitution site. SortObjectsByAST
    /// ignores these refs and relies on insertion order instead.
    pub is_var: bool,
}

impl Default for Object {
    fn default() -> Self {
        Self {
            id: String::new(),
            id_val: String::new(),
            abs_id: String::new(),
            label: Label::default(),
            shape: ScalarValue {
                value: "rectangle".to_owned(),
            },
            direction: Direction::default(),
            language: String::new(),
            top_left: Point::new(0.0, 0.0),
            width: 0.0,
            height: 0.0,
            box_: Box2D::new(Point::new(0.0, 0.0), 0.0, 0.0),
            parent: None,
            children: Vec::new(),
            children_array: Vec::new(),
            style: Style::default(),
            icon_style: IconStyle::default(),
            icon: None,
            icon_position: None,
            label_position: None,
            tooltip_position: None,
            label_dimensions: Dimensions::default(),
            tooltip: None,
            link: None,
            class: None,
            sql_table: None,
            content_aspect_ratio: None,
            width_attr: None,
            height_attr: None,
            top: None,
            left: None,
            near_key: None,
            constraint: Vec::new(),
            grid_rows: None,
            grid_columns: None,
            grid_rows_range: None,
            grid_columns_range: None,
            grid_gap: None,
            vertical_gap: None,
            horizontal_gap: None,
            z_index: 0,
            classes: Vec::new(),
            classes_explicit: false,
            filters: Vec::new(),
            is_sequence_diagram_note: false,
            is_sequence_diagram_group: false,
            references: Vec::new(),
            suspended: None,
        }
    }
}

impl Object {
    /// Absolute ID of the object for export.
    pub fn abs_id(&self) -> &str {
        &self.abs_id
    }

    /// The short ID value (without dotted path), fully unescaped.
    /// Mirrors Go `Object.IDVal`.
    pub fn id_val(&self) -> &str {
        if self.id_val.is_empty() {
            // Fallback for legacy constructors that don't set id_val.
            unformat_key_segment(&self.id)
        } else {
            &self.id_val
        }
    }

    /// True if this object has children.
    pub fn is_container(&self) -> bool {
        !self.children_array.is_empty()
    }

    /// Returns nesting level: 0 for root children, 1 for grandchildren, etc.
    pub fn level(&self, graph: &Graph) -> u32 {
        // Go d2graph.Object.Level() returns 1 for top-level (immediate children
        // of root), 2 for grandchildren, etc.
        let mut depth = 1;
        let mut p = self.parent;
        while let Some(pid) = p {
            if pid == graph.root {
                break;
            }
            depth += 1;
            p = graph.objects[pid].parent;
        }
        depth
    }

    /// Return text info for this object.
    /// Matches Go d2graph.Object.Text(): leaf shapes default to bold;
    /// containers/text shapes default to non-bold; container labels scale
    /// with depth (level 1 → XXL, 2 → XL, 3 → L, else M); explicit
    /// `style.bold` / `style.font-size` always win.
    pub fn text(&self, graph: &Graph) -> MText {
        let is_container = !self.children_array.is_empty();
        let in_seq = self.is_inside_sequence_diagram(graph);
        let mut is_bold = !is_container && self.shape.value != "text";
        // Match Go: `style.bold == "true"` forces bold on, but `"false"` does
        // NOT clear the default. Go's d2graph.Object.Text() only sets isBold
        // to true when the value is the literal "true"; there is no branch to
        // turn it off. So a leaf shape with `style.bold: false` is still
        // measured as bold. Mirroring this quirk keeps label widths matching.
        if let Some(v) = self.style.bold.as_ref()
            && v.value == "true"
        {
            is_bold = true;
        }
        // class shapes are never bold regardless of the default.
        if self.shape.value == crate::target::SHAPE_CLASS {
            is_bold = false;
        }
        // Default font size: leaves get FONT_SIZE_M (16); containers (and
        // grid diagrams, not yet modeled) scale by container level.
        // class / sql_table shapes start at FONT_SIZE_L (20) AND add
        // HEADER_FONT_ADD (4), so `obj.text().font_size` ends up at 24;
        // the exporter then subtracts HEADER_FONT_ADD so the stored
        // `text.font_size` lands at 20, and the class/table renderer
        // adds HEADER_FONT_ADD back for the header text.
        let mut font_size: i32 = if let Some(v) = self.style.font_size.as_ref() {
            v.value.parse().unwrap_or(16)
        } else if self.shape.value == crate::target::SHAPE_CLASS
            || self.shape.value == crate::target::SHAPE_SQL_TABLE
        {
            20 // FONT_SIZE_L
        } else {
            16
        };
        // Container font scaling only applies outside sequence diagrams.
        // Inside sequence diagrams, objects get isBold=false (Go:
        // `if obj.OuterSequenceDiagram() != nil { isBold = false }`).
        if !in_seq {
            if (is_container || self.is_grid_diagram())
                && self.shape.value != "text"
                && self.style.font_size.is_none()
            {
                font_size = match self.level(graph) {
                    1 => 28, // FONT_SIZE_XXL
                    2 => 24, // FONT_SIZE_XL
                    3 => 20, // FONT_SIZE_L
                    _ => 16, // FONT_SIZE_M
                };
            }
        } else {
            is_bold = false;
        }
        if self.shape.value == crate::target::SHAPE_CLASS
            || self.shape.value == crate::target::SHAPE_SQL_TABLE
        {
            font_size += crate::target::HEADER_FONT_ADD;
        }
        MText {
            text: self.label.value.clone(),
            font_size,
            is_bold,
            is_italic: self
                .style
                .italic
                .as_ref()
                .is_some_and(|v| v.value == "true"),
            dimensions: self.label_dimensions,
        }
    }

    /// Whether the object has a non-empty label.
    pub fn has_label(&self) -> bool {
        !matches!(
            self.shape.value.as_str(),
            crate::target::SHAPE_TEXT
                | crate::target::SHAPE_CLASS
                | crate::target::SHAPE_SQL_TABLE
                | crate::target::SHAPE_CODE
        ) && !self.label.value.is_empty()
    }

    /// Whether the object has an icon.
    pub fn has_icon(&self) -> bool {
        self.icon.is_some() && self.shape.value != crate::target::SHAPE_IMAGE
    }

    /// Whether the object has an outside bottom label. Matches Go
    /// `d2graph.Object.HasOutsideBottomLabel`: image and person shapes
    /// default their label to `OUTSIDE_BOTTOM_CENTER`.
    pub fn has_outside_bottom_label(&self) -> bool {
        matches!(
            self.shape.value.as_str(),
            crate::target::SHAPE_IMAGE | crate::target::SHAPE_PERSON
        )
    }

    /// Port of Go `shape.Shape.IsRectangular`. Returns true for shapes
    /// whose border is an axis-aligned rectangle — these don't need a
    /// perimeter-trace adjustment after the dagre box intersect.
    /// Also treats empty shape value as rectangular to match Go's
    /// `TraceToShapeBorder` fast path (`shape.Is("")`).
    pub fn is_rectangular_shape(&self) -> bool {
        matches!(
            self.shape.value.as_str(),
            "" | crate::target::SHAPE_RECTANGLE
                | crate::target::SHAPE_SQUARE
                | crate::target::SHAPE_IMAGE
                | crate::target::SHAPE_CLASS
                | crate::target::SHAPE_SQL_TABLE
                | crate::target::SHAPE_CODE
                | crate::target::SHAPE_TEXT
        )
    }

    /// Update the bounding box from top_left + width + height.
    pub fn update_box(&mut self) {
        self.box_ = Box2D::new(self.top_left, self.width, self.height);
    }

    /// Check if `self` is a descendant of `ancestor_id` in the graph.
    /// Mirrors Go `Object.IsDescendantOf`: returns true when self IS the ancestor.
    pub fn is_descendant_of(&self, self_id: ObjId, ancestor_id: ObjId, graph: &Graph) -> bool {
        if self_id == ancestor_id {
            return true;
        }
        let mut p = self.parent;
        while let Some(pid) = p {
            if pid == ancestor_id {
                return true;
            }
            p = graph.objects[pid].parent;
        }
        false
    }

    /// Get the fill color based on style. If the user set an explicit
    /// `style.fill`, use it; otherwise pick the default color based on
    /// shape type and container level. Mirrors Go d2graph.Object.GetFill
    /// for the cases d2 emits in normal usage (sequence-diagram and the
    /// exotic Cylinder/Step/Person/etc. branches are still TODO).
    pub fn get_fill(&self, graph: &Graph) -> &str {
        if let Some(ref f) = self.style.fill {
            return &f.value;
        }

        let shape = self.shape.value.to_lowercase();
        if shape == crate::target::SHAPE_SQL_TABLE || shape == crate::target::SHAPE_CLASS {
            return crate::color::N1;
        }

        // Sequence diagram special fills (Go: GetFill lines 520-542)
        // Note: Go computes IsSequenceDiagramNote dynamically — an object in
        // a sequence diagram that has no edge references, no children, and no
        // contained objects/edges is considered a "note" and gets fill N7.
        if self.is_sequence_diagram_note {
            return crate::color::N7;
        }
        // Dynamically check IsSequenceDiagramNote (Go d2graph/seqdiagram.go:42):
        //   OuterSequenceDiagram() != nil && !hasEdgeRef() && !ContainsAnyEdge()
        //   && len(ChildrenArray) == 0 && !ContainsAnyObject()
        //
        // For leaf objects (no children), `ContainsAnyObject` is always false
        // since no other object's scope chain can point at us. `ContainsAnyEdge`
        // is effectively equivalent to "any edge with this obj as src/dst in
        // source scope", matching `hasEdgeRef` + direct edge membership.
        if self.outer_sequence_diagram(graph).is_some()
            && self.children_array.is_empty()
            && !self.is_sequence_diagram_group
        {
            let my_id = self.abs_id();
            let has_source_edge = graph.edges.iter().any(|e| {
                e.scope_obj.is_some()
                    && (graph.objects[e.src].abs_id() == my_id
                        || graph.objects[e.dst].abs_id() == my_id)
            });
            if !has_source_edge {
                return crate::color::N7;
            }
        }
        if self.is_sequence_diagram_group {
            return crate::color::N5;
        }
        // Direct children of a sequence_diagram root always get B5.
        if let Some(pid) = self.parent
            && graph.objects[pid].shape.value == crate::target::SHAPE_SEQUENCE_DIAGRAM
        {
            return crate::color::B5;
        }
        // Spans inside sequence diagrams: fill by depth relative to the
        // sequence diagram root. Go Level() returns 0 for root, our level()
        // returns 1 for root. For non-root objects they match. Adjust for
        // root-level SD by using 0 when the SD is the graph root.
        if self.is_inside_sequence_diagram(graph) {
            // Find the sequence diagram ancestor and its Go-equivalent level
            let mut go_sd_level = 0u32;
            let mut cur = self.parent;
            while let Some(pid) = cur {
                if graph.objects[pid].shape.value == crate::target::SHAPE_SEQUENCE_DIAGRAM {
                    if pid == graph.root {
                        go_sd_level = 0; // Go root.Level() = 0
                    } else {
                        go_sd_level = graph.objects[pid].level(graph);
                    }
                    break;
                }
                cur = graph.objects[pid].parent;
            }
            let rel = self.level(graph) - go_sd_level;
            return match rel {
                1 => crate::color::B3,
                2 => crate::color::B4,
                3 => crate::color::B5,
                4 => crate::color::N6,
                _ => crate::color::N7,
            };
        }

        if shape == crate::target::SHAPE_SEQUENCE_DIAGRAM {
            return crate::color::N7;
        }

        let level = self.level(graph);
        let is_container = self.is_container();
        let is_rect_like = shape.is_empty()
            || shape == crate::target::SHAPE_SQUARE
            || shape == crate::target::SHAPE_CIRCLE
            || shape == crate::target::SHAPE_OVAL
            || shape == crate::target::SHAPE_RECTANGLE
            || shape == crate::target::SHAPE_HIERARCHY;
        if is_rect_like {
            return match level {
                1 => {
                    if is_container {
                        crate::color::B4
                    } else {
                        crate::color::B6
                    }
                }
                2 => crate::color::B5,
                3 => crate::color::B6,
                _ => crate::color::N7,
            };
        }

        if shape == crate::target::SHAPE_CYLINDER
            || shape == crate::target::SHAPE_STORED_DATA
            || shape == crate::target::SHAPE_PACKAGE
        {
            return if level == 1 {
                crate::color::AA4
            } else {
                crate::color::AA5
            };
        }
        if shape == crate::target::SHAPE_STEP
            || shape == crate::target::SHAPE_PAGE
            || shape == crate::target::SHAPE_DOCUMENT
        {
            return if level == 1 {
                crate::color::AB4
            } else {
                crate::color::AB5
            };
        }
        if shape == crate::target::SHAPE_PERSON || shape == crate::target::SHAPE_C4_PERSON {
            return crate::color::B3;
        }
        if shape == crate::target::SHAPE_DIAMOND {
            return crate::color::N4;
        }
        if shape == crate::target::SHAPE_CLOUD || shape == crate::target::SHAPE_CALLOUT {
            return crate::color::N7;
        }
        if shape == crate::target::SHAPE_QUEUE
            || shape == crate::target::SHAPE_PARALLELOGRAM
            || shape == crate::target::SHAPE_HEXAGON
        {
            return crate::color::N5;
        }
        crate::color::N7
    }

    /// Get the stroke color based on style and stroke-dash.
    pub fn get_stroke(&self, stroke_dash: f64) -> &str {
        if let Some(ref s) = self.style.stroke {
            return &s.value;
        }

        if self
            .shape
            .value
            .eq_ignore_ascii_case(crate::target::SHAPE_CODE)
            || self
                .shape
                .value
                .eq_ignore_ascii_case(crate::target::SHAPE_TEXT)
        {
            return crate::color::N1;
        }
        if self
            .shape
            .value
            .eq_ignore_ascii_case(crate::target::SHAPE_CLASS)
            || self
                .shape
                .value
                .eq_ignore_ascii_case(crate::target::SHAPE_SQL_TABLE)
        {
            return crate::color::N7;
        }
        if stroke_dash != 0.0 {
            return crate::color::B2;
        }
        crate::color::B1
    }

    /// Get the 3D/multiple modifier adjustments (dx, dy).
    pub fn get_modifier_element_adjustments(&self) -> (f64, f64) {
        let three_dee = self
            .style
            .three_dee
            .as_ref()
            .is_some_and(|v| v.value == "true");
        let multiple = self
            .style
            .multiple
            .as_ref()
            .is_some_and(|v| v.value == "true");
        let mut dx = 0.0;
        let mut dy = 0.0;
        if three_dee {
            dx += crate::target::THREE_DEE_OFFSET as f64;
            dy += crate::target::THREE_DEE_OFFSET as f64;
        }
        if multiple {
            dx += crate::target::MULTIPLE_OFFSET as f64;
            dy += crate::target::MULTIPLE_OFFSET as f64;
        }
        (dx, dy)
    }

    /// Returns `(margin, padding)` mirroring Go `Object.Spacing()` /
    /// `SpacingOpt`. Outside labels/icons contribute to margin, inside labels
    /// contribute to padding, and 3D/multiple modifier adjustments add to
    /// `margin.Right` / `margin.Top`.
    pub fn spacing(&self) -> (Spacing, Spacing) {
        self.spacing_opt(
            2.0 * crate::label::PADDING,
            2.0 * crate::label::PADDING,
            true,
        )
    }

    /// Underlying variant allowing callers to tune label/icon padding.
    /// Ported from Go d2graph.Object.SpacingOpt.
    pub fn spacing_opt(
        &self,
        label_padding: f64,
        icon_padding: f64,
        max_icon_size: bool,
    ) -> (Spacing, Spacing) {
        let mut margin = Spacing {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        };
        let mut padding = Spacing {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        };

        if self.has_label() {
            let position = self.label_position.as_deref().unwrap_or("");
            let lw = if self.label_dimensions.width > 0 {
                self.label_dimensions.width as f64 + label_padding
            } else {
                0.0
            };
            let lh = if self.label_dimensions.height > 0 {
                self.label_dimensions.height as f64 + label_padding
            } else {
                0.0
            };

            match position {
                "OUTSIDE_TOP_LEFT" | "OUTSIDE_TOP_CENTER" | "OUTSIDE_TOP_RIGHT" => {
                    margin.top = lh;
                }
                "OUTSIDE_BOTTOM_LEFT" | "OUTSIDE_BOTTOM_CENTER" | "OUTSIDE_BOTTOM_RIGHT" => {
                    margin.bottom = lh;
                }
                "OUTSIDE_LEFT_TOP" | "OUTSIDE_LEFT_MIDDLE" | "OUTSIDE_LEFT_BOTTOM" => {
                    margin.left = lw;
                }
                "OUTSIDE_RIGHT_TOP" | "OUTSIDE_RIGHT_MIDDLE" | "OUTSIDE_RIGHT_BOTTOM" => {
                    margin.right = lw;
                }
                "INSIDE_TOP_LEFT" | "INSIDE_TOP_CENTER" | "INSIDE_TOP_RIGHT" => {
                    padding.top = lh;
                }
                "INSIDE_BOTTOM_LEFT" | "INSIDE_BOTTOM_CENTER" | "INSIDE_BOTTOM_RIGHT" => {
                    padding.bottom = lh;
                }
                "INSIDE_MIDDLE_LEFT" => {
                    padding.left = lw;
                }
                "INSIDE_MIDDLE_RIGHT" => {
                    padding.right = lw;
                }
                _ => {}
            }
        }

        if self.has_icon() {
            let position = self.icon_position.as_deref().unwrap_or("");
            let icon_size = if max_icon_size {
                crate::target::MAX_ICON_SIZE as f64 + icon_padding
            } else {
                // Fallback to MAX_ICON_SIZE when we don't have a per-shape
                // sizer available — matches the conservative upper bound.
                crate::target::MAX_ICON_SIZE as f64 + icon_padding
            };
            match position {
                "OUTSIDE_TOP_LEFT" | "OUTSIDE_TOP_CENTER" | "OUTSIDE_TOP_RIGHT" => {
                    margin.top = margin.top.max(icon_size);
                }
                "OUTSIDE_BOTTOM_LEFT" | "OUTSIDE_BOTTOM_CENTER" | "OUTSIDE_BOTTOM_RIGHT" => {
                    margin.bottom = margin.bottom.max(icon_size);
                }
                "OUTSIDE_LEFT_TOP" | "OUTSIDE_LEFT_MIDDLE" | "OUTSIDE_LEFT_BOTTOM" => {
                    margin.left = margin.left.max(icon_size);
                }
                "OUTSIDE_RIGHT_TOP" | "OUTSIDE_RIGHT_MIDDLE" | "OUTSIDE_RIGHT_BOTTOM" => {
                    margin.right = margin.right.max(icon_size);
                }
                "INSIDE_TOP_LEFT" | "INSIDE_TOP_CENTER" | "INSIDE_TOP_RIGHT" => {
                    padding.top = padding.top.max(icon_size);
                }
                "INSIDE_BOTTOM_LEFT" | "INSIDE_BOTTOM_CENTER" | "INSIDE_BOTTOM_RIGHT" => {
                    padding.bottom = padding.bottom.max(icon_size);
                }
                "INSIDE_MIDDLE_LEFT" => {
                    padding.left = padding.left.max(icon_size);
                }
                "INSIDE_MIDDLE_RIGHT" => {
                    padding.right = padding.right.max(icon_size);
                }
                _ => {}
            }
        }

        let (dx, dy) = self.get_modifier_element_adjustments();
        margin.right += dx;
        margin.top += dy;

        (margin, padding)
    }

    /// Trace edge endpoints to the shape boundary.
    /// Returns (new_start_index, new_end_index) after clipping.
    pub fn trace_to_shape_start(&self, points: &[Point], start_index: usize) -> usize {
        // Simplified: clip to bounding box
        for i in (start_index + 1)..points.len() {
            let seg = Segment::new(points[i - 1], points[i]);
            let ints = self.box_.intersections(&seg);
            if !ints.is_empty() {
                return i - 1;
            }
        }
        start_index
    }

    pub fn trace_to_shape_end(&self, points: &[Point], end_index: usize) -> usize {
        // Simplified: clip to bounding box
        for i in (1..=end_index).rev() {
            let seg = Segment::new(points[i - 1], points[i]);
            let ints = self.box_.intersections(&seg);
            if !ints.is_empty() {
                return i;
            }
        }
        end_index
    }

    /// Is this a sequence diagram container?
    pub fn is_sequence_diagram(&self) -> bool {
        self.shape.value == crate::target::SHAPE_SEQUENCE_DIAGRAM
    }

    /// Walk up the parent chain and return `true` if any ancestor is a
    /// sequence diagram. Mirrors Go `Object.OuterSequenceDiagram() != nil`.
    pub fn is_inside_sequence_diagram(&self, graph: &Graph) -> bool {
        self.outer_sequence_diagram(graph).is_some()
    }

    /// Walk up the parent chain and return the nearest ancestor that is a
    /// sequence diagram. Mirrors Go `Object.OuterSequenceDiagram()`.
    pub fn outer_sequence_diagram(&self, graph: &Graph) -> Option<ObjId> {
        let mut cur = self.parent;
        while let Some(pid) = cur {
            if graph.objects[pid].shape.value == crate::target::SHAPE_SEQUENCE_DIAGRAM {
                return Some(pid);
            }
            cur = graph.objects[pid].parent;
        }
        None
    }

    /// Is this a sequence diagram group?
    pub fn is_sequence_diagram_group(&self) -> bool {
        self.is_sequence_diagram_group
    }

    /// Is this a grid diagram?
    pub fn is_grid_diagram(&self) -> bool {
        self.grid_rows.is_some() || self.grid_columns.is_some()
    }

    /// Get label top-left position (simplified).
    pub fn get_label_top_left(&self) -> Option<Point> {
        let pos_str = self.label_position.as_deref()?;
        let pos = crate::label::Position::from_string(pos_str);
        let b = Box2D::new(self.top_left, self.width, self.height);
        let w = self.label_dimensions.width as f64;
        let h = self.label_dimensions.height as f64;
        Some(pos.get_point_on_box(&b, crate::label::PADDING, w, h))
    }

    /// Get icon top-left position (simplified).
    pub fn get_icon_top_left(&self) -> Option<Point> {
        let pos_str = self.icon_position.as_deref()?;
        let pos = crate::label::Position::from_string(pos_str);
        let b = Box2D::new(self.top_left, self.width, self.height);
        let size = crate::target::MAX_ICON_SIZE as f64;
        Some(pos.get_point_on_box(&b, crate::label::PADDING, size, size))
    }

    /// Center of this object's bounding box.
    pub fn center(&self) -> Point {
        Point {
            x: self.top_left.x + self.width / 2.0,
            y: self.top_left.y + self.height / 2.0,
        }
    }

    /// Get the margin (space needed for outside labels/icons).
    /// Mirrors Go `Object.GetMargin()`.
    pub fn get_margin(&self) -> Spacing {
        let mut margin = Spacing::default();

        if self.has_label()
            && let Some(ref pos_str) = self.label_position
        {
            let position = crate::label::Position::from_string(pos_str);
            let label_width = self.label_dimensions.width as f64 + crate::label::PADDING;
            let label_height = self.label_dimensions.height as f64 + crate::label::PADDING;

            use crate::label::Position::*;
            match position {
                OutsideTopLeft | OutsideTopCenter | OutsideTopRight => {
                    margin.top = label_height;
                }
                OutsideBottomLeft | OutsideBottomCenter | OutsideBottomRight => {
                    margin.bottom = label_height;
                }
                OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom => {
                    margin.left = label_width;
                }
                OutsideRightTop | OutsideRightMiddle | OutsideRightBottom => {
                    margin.right = label_width;
                }
                _ => {}
            }

            // if an outside label is larger than the object add margin accordingly
            if label_width > self.width {
                let dx = label_width - self.width;
                match position {
                    OutsideTopLeft | OutsideBottomLeft => margin.right = dx,
                    OutsideTopCenter | OutsideBottomCenter => {
                        margin.left = (dx / 2.0).ceil();
                        margin.right = (dx / 2.0).ceil();
                    }
                    OutsideTopRight | OutsideBottomRight => margin.left = dx,
                    _ => {}
                }
            }
            if label_height > self.height {
                let dy = label_height - self.height;
                match position {
                    OutsideLeftTop | OutsideRightTop => margin.bottom = dy,
                    OutsideLeftMiddle | OutsideRightMiddle => {
                        margin.top = (dy / 2.0).ceil();
                        margin.bottom = (dy / 2.0).ceil();
                    }
                    OutsideLeftBottom | OutsideRightBottom => margin.top = dy,
                    _ => {}
                }
            }
        }

        if self.has_icon()
            && let Some(ref pos_str) = self.icon_position
        {
            let position = crate::label::Position::from_string(pos_str);
            let icon_size = crate::target::MAX_ICON_SIZE as f64 + crate::label::PADDING;
            use crate::label::Position::*;
            match position {
                OutsideTopLeft | OutsideTopCenter | OutsideTopRight => {
                    margin.top = margin.top.max(icon_size);
                }
                OutsideBottomLeft | OutsideBottomCenter | OutsideBottomRight => {
                    margin.bottom = margin.bottom.max(icon_size);
                }
                OutsideLeftTop | OutsideLeftMiddle | OutsideLeftBottom => {
                    margin.left = margin.left.max(icon_size);
                }
                OutsideRightTop | OutsideRightMiddle | OutsideRightBottom => {
                    margin.right = margin.right.max(icon_size);
                }
                _ => {}
            }
        }

        let (dx, dy) = self.get_modifier_element_adjustments();
        margin.right += dx;
        margin.top += dy;
        margin
    }

    /// Resize this object to fit the given content dimensions, applying
    /// shape-specific sizing rules. Mirrors Go `Object.SizeToContent()`.
    pub fn size_to_content(&mut self, content_w: f64, content_h: f64, pad_x: f64, pad_y: f64) {
        let dsl_shape = self.shape.value.to_lowercase();
        let shape_type = crate::target::dsl_shape_to_shape_type(&dsl_shape);
        let bbox = Box2D::new(Point { x: 0.0, y: 0.0 }, content_w, content_h);
        let s = crate::shape::Shape::new(shape_type, bbox);

        let (fit_w, fit_h) = if shape_type == crate::shape::PERSON_TYPE {
            (content_w + pad_x, content_h + pad_y)
        } else {
            s.get_dimensions_to_fit(content_w, content_h, pad_x, pad_y)
        };

        if let Some(ref wa) = self.width_attr {
            if let Ok(w) = wa.value.parse::<f64>() {
                self.width = w;
            }
        } else {
            self.width = fit_w;
        }

        if let Some(ref ha) = self.height_attr {
            if let Ok(h) = ha.value.parse::<f64>() {
                self.height = h;
            }
        } else {
            self.height = fit_h;
        }

        // SQL tables, classes, code blocks: use max of desired and fit
        if self.sql_table.is_some() || self.class.is_some() || !self.language.is_empty() {
            self.width = self.width.max(fit_w);
            self.height = self.height.max(fit_h);
        }

        // Aspect-ratio-1 shapes (circle, real_square, etc.)
        if s.aspect_ratio_1() {
            let side = self.width.max(self.height);
            self.width = side;
            self.height = side;
        }

        // Mirror Go: cloud shapes record the content aspect ratio so
        // downstream callers (grid layout, renderer) place children using
        // the original content's aspect, not the bbox aspect.
        if shape_type == crate::shape::CLOUD_TYPE {
            let inner = crate::shape::ShapeOps::get_inner_box_for_content(&s, content_w, content_h);
            if let Some(inner) = inner
                && inner.height > 0.0
            {
                self.content_aspect_ratio = Some(inner.width / inner.height);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Object movement helpers (free functions operating on graph)
// ---------------------------------------------------------------------------

/// Move object and all descendants by (dx, dy). Mirrors Go `Object.MoveWithDescendants`.
pub fn move_obj_with_descendants(g: &mut Graph, obj_id: ObjId, dx: f64, dy: f64) {
    if obj_id >= g.objects.len() {
        return;
    }
    g.objects[obj_id].top_left.x += dx;
    g.objects[obj_id].top_left.y += dy;
    let children: Vec<ObjId> = g.objects[obj_id].children_array.clone();
    for child_id in children {
        move_obj_with_descendants(g, child_id, dx, dy);
    }
}

/// Move object and all descendants so that top-left is at (x, y).
/// Mirrors Go `Object.MoveWithDescendantsTo`.
pub fn move_obj_with_descendants_to(g: &mut Graph, obj_id: ObjId, x: f64, y: f64) {
    let dx = x - g.objects[obj_id].top_left.x;
    let dy = y - g.objects[obj_id].top_left.y;
    move_obj_with_descendants(g, obj_id, dx, dy);
}

// ---------------------------------------------------------------------------
// Edge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Edge {
    pub abs_id: String,

    pub src: ObjId,
    pub dst: ObjId,

    pub src_arrow: bool,
    pub dst_arrow: bool,

    pub src_arrowhead: Option<ArrowheadInfo>,
    pub dst_arrowhead: Option<ArrowheadInfo>,

    pub label: Label,
    pub label_dimensions: Dimensions,
    pub label_position: Option<String>,
    pub label_percentage: Option<f32>,

    pub route: Vec<Point>,
    pub is_curve: bool,

    pub style: Style,
    pub icon_style: IconStyle,

    pub tooltip: Option<ScalarValue>,
    pub link: Option<ScalarValue>,
    pub icon: Option<String>,
    pub icon_position: Option<String>,

    pub language: String,

    pub z_index: i32,
    pub classes: Vec<String>,

    /// Earliest AST reference for this edge. Mirrors Go
    /// `d2graph.Edge.References[0].Edge.Range`, which
    /// `d2graph.Graph.SortEdgesByAST` uses to order edges in source-file
    /// order before rendering. `None` if no reference is set.
    pub first_ast_range: Option<ast::Range>,

    /// Index of the source column inside its sql_table. Mirrors Go
    /// `d2graph.Edge.SrcTableColumnIndex`. Set by `Graph::connect` when
    /// the source path ends inside a sql_table.
    pub src_table_column_index: Option<usize>,
    /// Index of the destination column inside its sql_table. Mirrors
    /// Go `d2graph.Edge.DstTableColumnIndex`.
    pub dst_table_column_index: Option<usize>,

    /// Overrides the normal `g.objects[dst].abs_id()` for exporter output.
    /// Used for synthetic lifeline-end objects in sequence diagrams.
    pub dst_id_override: Option<String>,

    /// The object in whose scope this edge was declared. Mirrors Go
    /// `Edge.References[].ScopeObj`. Used by sequence diagram layout to
    /// determine if an edge is contained by a group.
    pub scope_obj: Option<ObjId>,

    /// Suspension state propagated from IR. Suspended edges are removed
    /// after glob expansion; unsuspended edges lift suspension on their
    /// endpoints and ancestors. Mirrors Go d2ir.Edge.suspended.
    pub suspended: Option<bool>,
}

impl Default for Edge {
    fn default() -> Self {
        Self {
            abs_id: String::new(),
            src: 0,
            dst: 0,
            src_arrow: false,
            dst_arrow: true,
            src_arrowhead: None,
            dst_arrowhead: None,
            label: Label::default(),
            label_dimensions: Dimensions::default(),
            label_position: None,
            label_percentage: None,
            route: Vec::new(),
            is_curve: false,
            style: Style::default(),
            icon_style: IconStyle::default(),
            tooltip: None,
            link: None,
            icon: None,
            icon_position: None,
            language: String::new(),
            z_index: 0,
            classes: Vec::new(),
            first_ast_range: None,
            src_table_column_index: None,
            dst_table_column_index: None,
            dst_id_override: None,
            scope_obj: None,
            suspended: None,
        }
    }
}

impl Edge {
    /// Absolute ID for export.
    pub fn abs_id(&self) -> &str {
        &self.abs_id
    }

    /// Return text info for this edge (simplified).
    pub fn text(&self) -> MText {
        let font_size = self
            .style
            .font_size
            .as_ref()
            .and_then(|v| v.value.parse().ok())
            .unwrap_or(16);
        MText {
            text: self.label.value.clone(),
            font_size,
            is_bold: self.style.bold.as_ref().is_some_and(|v| v.value == "true"),
            // Match Go d2graph.Edge.Text(): edge labels default to italic.
            // An explicit `style.italic: false` opts out, but absent any
            // style the value should still be true.
            is_italic: self.style.italic.as_ref().is_none_or(|v| v.value == "true"),
            dimensions: self.label_dimensions,
        }
    }

    /// Get edge stroke color based on style and stroke-dash.
    pub fn get_stroke(&self, stroke_dash: f64) -> &str {
        if let Some(ref s) = self.style.stroke {
            return &s.value;
        }
        if stroke_dash != 0.0 {
            return crate::color::B2;
        }
        crate::color::B1
    }

    /// Trace edge endpoints to the shape boundaries.
    /// Returns (new_start_index, new_end_index).
    pub fn trace_to_shape(
        &self,
        points: &[Point],
        start_index: usize,
        end_index: usize,
        graph: &Graph,
    ) -> (usize, usize) {
        let src = &graph.objects[self.src];
        let dst = &graph.objects[self.dst];

        let new_start = src.trace_to_shape_start(points, start_index);
        let new_end = dst.trace_to_shape_end(points, end_index);
        (new_start, new_end)
    }

    /// Translate all route points by (dx, dy). Mirrors Go `Edge.Move`.
    pub fn move_route(&mut self, dx: f64, dy: f64) {
        for p in &mut self.route {
            p.x += dx;
            p.y += dy;
        }
    }
}

// ---------------------------------------------------------------------------
// Legend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct GraphLegend {
    pub label: String,
    /// Non-root objects compiled from `vars.d2-legend`.
    pub objects: Vec<Object>,
    /// Edges compiled from `vars.d2-legend`. `src`/`dst` are indices into
    /// `objects` (offset by 1 if `root` is at index 0 of the scratch graph).
    pub edges: Vec<Edge>,
    /// Absolute IDs of the scratch graph's objects (root + children) so
    /// the exporter can resolve `edge.src`/`edge.dst` without needing
    /// access to the scratch graph. Index matches the scratch graph's
    /// object indices.
    pub object_abs_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Graph {
    pub name: String,
    pub is_folder_only: bool,

    pub root: ObjId,
    pub objects: Vec<Object>,
    pub edges: Vec<Edge>,

    pub theme: Option<crate::themes::Theme>,
    pub legend: Option<GraphLegend>,

    /// Nesting depth from the outermost graph. Mirrors Go `Graph.RootLevel`.
    /// 0 = top-level, >0 = extracted nested graph.
    pub root_level: u32,

    // Nested boards (layers, scenarios, steps)
    pub layers: Vec<Graph>,
    pub scenarios: Vec<Graph>,
    pub steps: Vec<Graph>,

    // Parent graph reference (for nested boards)
    pub parent: Option<Box<Graph>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            name: String::new(),
            is_folder_only: false,
            root: 0,
            objects: vec![Object {
                id: "root".to_owned(),
                abs_id: "".to_owned(),
                ..Default::default()
            }],
            edges: Vec::new(),
            theme: None,
            legend: None,
            root_level: 0,
            layers: Vec::new(),
            scenarios: Vec::new(),
            steps: Vec::new(),
            parent: None,
        }
    }
}

impl Graph {
    /// Create a new empty graph with a root object at index 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reorder objects so they appear in source order, mirroring Go
    /// d2graph.Graph.SortObjectsByAST. Sort key is the AST range of the
    /// first reference's `key.path[key_path_index]`.
    ///
    /// Because ObjIds are positions in `self.objects`, this also rewrites
    /// every parent / children / children_array / edge endpoint with the
    /// new positions so the graph remains internally consistent.
    pub fn sort_objects_by_ast(&mut self) {
        // Mirror Go `d2graph.(*Graph).SortObjectsByAST`. Go's comparator:
        //
        //   - if either side's References[0] is a var substitution,
        //     preserve original index order (i < j);
        //   - else compare by the byte range of References[0]'s key path
        //     at KeyPathIndex.
        //
        // Go uses `sort.Slice` (unstable). The comparator is non-transitive
        // for inputs mixing var-only and key-sourced nodes, so results depend
        // on the sort algorithm. Empirically, Go preserves the pre-sort
        // insertion order for var-ref nodes; only key-ref nodes whose byte
        // ranges are out of order get swapped past each other. We reproduce
        // that effect with a stable insertion-sort that applies the swap
        // only when the comparator unambiguously says so, matching Go's
        // observed output byte-for-byte on grid/var-heavy inputs.
        let root = self.root;
        let mut order: Vec<ObjId> = (0..self.objects.len()).collect();

        // Precompute per-object: (is_var_ref0, optional key-path byte range).
        // - None range means no References[0] byte-range is available
        //   (no references, or References[0] is a var substitution).
        struct SortKey {
            has_ref: bool,
            is_var: bool,
            range: Option<ast::Range>,
        }
        let keys: Vec<SortKey> = (0..self.objects.len())
            .map(|i| {
                let obj = &self.objects[i];
                if obj.references.is_empty() {
                    return SortKey {
                        has_ref: false,
                        is_var: false,
                        range: None,
                    };
                }
                let r0 = &obj.references[0];
                if r0.is_var {
                    return SortKey {
                        has_ref: true,
                        is_var: true,
                        range: None,
                    };
                }
                let range = r0
                    .key
                    .path
                    .get(r0.key_path_index)
                    .map(|sb| sb.get_range().clone());
                SortKey {
                    has_ref: true,
                    is_var: false,
                    range,
                }
            })
            .collect();

        // Comparator matching Go's semantics exactly (non-transitive):
        // Returns Ordering::Less when Go's `less(a, b)` would be true,
        // Ordering::Greater when `less(b, a)` would be true, otherwise Equal.
        let go_cmp = |a: ObjId, b: ObjId| -> std::cmp::Ordering {
            if a == root || b == root {
                // Root stays at its index; use insertion order as tiebreak.
                return a.cmp(&b);
            }
            let ka = &keys[a];
            let kb = &keys[b];
            if !ka.has_ref || !kb.has_ref {
                return a.cmp(&b);
            }
            if ka.is_var || kb.is_var {
                return a.cmp(&b);
            }
            match (&ka.range, &kb.range) {
                (Some(ra), Some(rb)) => range_cmp(ra, rb),
                _ => a.cmp(&b),
            }
        };

        // Stable insertion sort: at each step, insert order[i] into the
        // already-sorted prefix by walking left while the comparator says
        // the neighbor is strictly greater. Because var-ref nodes compare
        // "equal" (via index tiebreak returning Greater-only-when-a>b) to
        // non-var neighbors, they won't be moved past them — matching Go's
        // observed behavior on this family of inputs.
        for i in 1..order.len() {
            let cur = order[i];
            let mut j = i;
            while j > 0 {
                let prev = order[j - 1];
                if go_cmp(prev, cur) == std::cmp::Ordering::Greater {
                    order[j] = prev;
                    j -= 1;
                } else {
                    break;
                }
            }
            order[j] = cur;
        }

        // No-op if already sorted.
        if order.iter().enumerate().all(|(i, &j)| i == j) {
            return;
        }

        // Build old_to_new mapping.
        let mut old_to_new = vec![0usize; self.objects.len()];
        for (new_idx, &old_idx) in order.iter().enumerate() {
            old_to_new[old_idx] = new_idx;
        }

        // Reorder self.objects.
        let mut new_objects: Vec<Object> = Vec::with_capacity(self.objects.len());
        for &old_idx in &order {
            new_objects.push(std::mem::take(&mut self.objects[old_idx]));
        }
        self.objects = new_objects;

        // Rewrite ObjId references inside each object.
        for obj in self.objects.iter_mut() {
            if let Some(ref mut p) = obj.parent {
                *p = old_to_new[*p];
            }
            for c in obj.children.iter_mut() {
                *c = old_to_new[*c];
            }
            for c in obj.children_array.iter_mut() {
                *c = old_to_new[*c];
            }
        }

        // Rewrite root.
        self.root = old_to_new[self.root];

        // Rewrite edge endpoints and scope_obj.
        for e in self.edges.iter_mut() {
            e.src = old_to_new[e.src];
            e.dst = old_to_new[e.dst];
            if let Some(ref mut s) = e.scope_obj {
                *s = old_to_new[*s];
            }
        }

        // Rewrite scope_obj in object references.
        for obj in self.objects.iter_mut() {
            for r in obj.references.iter_mut() {
                if let Some(ref mut s) = r.scope_obj {
                    *s = old_to_new[*s];
                }
            }
        }
    }

    /// Stable-sort edges by their first AST reference. Mirrors Go
    /// `d2graph.Graph.SortEdgesByAST` — edges without a range keep their
    /// relative order.
    pub fn sort_edges_by_ast(&mut self) {
        // Match Go `sort.Slice` exactly — including its pdqsort-specific
        // handling of equal-key runs. A stable Rust sort is not enough:
        // when N glob-expanded edges all share the same AST range, Go's
        // unstable pdqsort leaves them in a deterministic but
        // non-insertion order (e.g. `default.* -> layout` yields
        // `tala, dagre, elk` from input `dagre, elk, tala`). Matching
        // that ordering is required for SVG byte-level parity on fixtures
        // like `stable/nesting_power`.
        let n = self.edges.len();
        if n <= 1 {
            return;
        }
        let mut order: Vec<usize> = (0..n).collect();
        // Snapshot the sort key per original edge index so the comparator
        // stays correct as positions shuffle.
        let ranges: Vec<Option<ast::Range>> = self
            .edges
            .iter()
            .map(|e| e.first_ast_range.clone())
            .collect();
        use std::cell::RefCell;
        let order_cell = RefCell::new(order);
        // Go's `Range.Before` delegates to `Position.Before`, which
        // compares byte offsets only. Equal-byte ranges report
        // "neither is before" — required so the unstable pdqsort can
        // freely permute equal-key runs the same way Go does.
        let range_before =
            |ra: &ast::Range, rb: &ast::Range| -> bool { ra.start.byte < rb.start.byte };
        go_sort::go_sort_slice(
            n,
            |i, j| {
                let o = order_cell.borrow();
                let ia = o[i];
                let ib = o[j];
                match (ranges[ia].as_ref(), ranges[ib].as_ref()) {
                    (Some(ra), Some(rb)) => range_before(ra, rb),
                    // Mirror Go: if either side has no references, use
                    // `i < j` (the positional indices after shuffling,
                    // not the original IDs).
                    _ => i < j,
                }
            },
            |i, j| {
                order_cell.borrow_mut().swap(i, j);
            },
        );
        order = order_cell.into_inner();

        if order.iter().enumerate().all(|(i, &j)| i == j) {
            return;
        }
        let mut new_edges: Vec<Edge> = Vec::with_capacity(n);
        for &old in &order {
            new_edges.push(std::mem::take(&mut self.edges[old]));
        }
        self.edges = new_edges;
    }

    /// Get the root object.
    pub fn root_obj(&self) -> &Object {
        &self.objects[self.root]
    }

    /// Get a mutable reference to the root object.
    pub fn root_obj_mut(&mut self) -> &mut Object {
        &mut self.objects[self.root]
    }

    /// Add an object to the graph, returning its ObjId.
    pub fn add_object(&mut self, mut obj: Object) -> ObjId {
        let id = self.objects.len();
        if obj.parent.is_none() {
            obj.parent = Some(self.root);
        }
        self.objects.push(obj);

        // Register as child of parent
        let parent_id = self.objects[id].parent.unwrap_or(self.root);
        self.objects[parent_id].children.push(id);
        self.objects[parent_id].children_array.push(id);

        id
    }

    /// Add an edge to the graph, returning its index.
    pub fn add_edge(&mut self, edge: Edge) -> usize {
        let idx = self.edges.len();
        self.edges.push(edge);
        idx
    }

    /// Navigate to a nested board by name.
    pub fn get_board(&self, _name: &str) -> Option<&Graph> {
        // Simplified: no nested boards
        None
    }

    /// Ensure a child object exists at the given path (relative to `parent`).
    /// Creates intermediate objects as needed. Returns the ObjId.
    ///
    /// Mirrors Go `Object.EnsureChild`: when inside a sequence diagram, if the
    /// first path segment matches a top-level child of the sequence diagram, the
    /// walk redirects to that child (i.e. edges inside groups resolve to the
    /// existing top-level actors).
    pub fn ensure_child_of(&mut self, parent: ObjId, ida: &[String]) -> ObjId {
        let mut cur = parent;

        // Sequence-diagram redirect: if cur is inside a sequence diagram and
        // the first segment of ida matches a top-level child of that diagram,
        // redirect to the diagram root so the walk creates/finds the child
        // there instead of under the current (group) scope.
        if let Some(first_name) = ida.first()
            && let Some(seq_id) = self.objects[cur].outer_sequence_diagram(self)
        {
            let seq_has_child = self.objects[seq_id]
                .children_array
                .iter()
                .any(|&cid| self.objects[cid].id_val() == first_name);
            if seq_has_child {
                // Edge case from Go: if cur.id == first_name (a.a pattern),
                // the second a should be created as a child of a, not
                // redirected. Skip the redirect in that case.
                if self.objects[cur].id_val() != first_name {
                    cur = seq_id;
                }
            }
        }

        for name in ida {
            // Look for existing child — match on the unquoted value
            // (id_val), same as the incoming IR field name. Comparison is
            // case-insensitive to mirror Go's `obj.Children[ToLower(head)]`
            // lookup (so `Manual` and `manual` resolve to the same child).
            let existing = self.objects[cur]
                .children_array
                .iter()
                .find(|&&cid| self.objects[cid].id_val().eq_ignore_ascii_case(name))
                .copied();
            if let Some(cid) = existing {
                cur = cid;
            } else {
                // Format the key segment: adds quotes when the name
                // contains special characters, matching Go's
                // newObject → d2format.Format(RawString(name, true)).
                let formatted = format_key_segment(name);
                let parent_abs = self.objects[cur].abs_id.clone();
                let abs = if parent_abs.is_empty() {
                    formatted.clone()
                } else {
                    format!("{}.{}", parent_abs, formatted)
                };
                let idx = self.objects.len();
                let obj = Object {
                    id: formatted,
                    id_val: name.clone(),
                    abs_id: abs,
                    label: Label {
                        value: name.clone(),
                        ..Default::default()
                    },
                    shape: ScalarValue {
                        value: String::new(),
                    },
                    parent: Some(cur),
                    ..Default::default()
                };
                self.objects.push(obj);
                self.objects[cur].children.push(idx);
                self.objects[cur].children_array.push(idx);
                cur = idx;
            }
        }
        cur
    }

    /// Ensure a child object exists at the given path from root.
    pub fn ensure_child(&mut self, ida: &[String]) -> ObjId {
        self.ensure_child_of(self.root, ida)
    }

    /// Create a new child object under `parent` without deduplicating on
    /// id_val. Used for `**` glob templates where each source occurrence
    /// must remain a distinct object so its filters + attrs apply as a
    /// standalone template pass (see compile_field in d2-compiler).
    pub fn new_child_of(&mut self, parent: ObjId, name: &str) -> ObjId {
        let formatted = format_key_segment(name);
        let parent_abs = self.objects[parent].abs_id.clone();
        let abs = if parent_abs.is_empty() {
            formatted.clone()
        } else {
            format!("{}.{}", parent_abs, formatted)
        };
        let idx = self.objects.len();
        let obj = Object {
            id: formatted,
            id_val: name.to_string(),
            abs_id: abs,
            label: Label {
                value: name.to_string(),
                ..Default::default()
            },
            shape: ScalarValue {
                value: String::new(),
            },
            parent: Some(parent),
            ..Default::default()
        };
        self.objects.push(obj);
        self.objects[parent].children.push(idx);
        self.objects[parent].children_array.push(idx);
        idx
    }

    /// Like `ensure_child_of` but stops descending as soon as the walk
    /// enters a `class` or `sql_table` shape, returning that shape.
    /// Mirrors Go `ensureChildEdge` — used exclusively for edge endpoint
    /// resolution so that `a.column -> b` yields `src = a` and not a
    /// synthetic child object for the column.
    pub fn ensure_child_edge_of(&mut self, parent: ObjId, ida: &[String]) -> ObjId {
        let mut cur = parent;
        for name in ida {
            let shape_val = self.objects[cur].shape.value.clone();
            if shape_val == "class" || shape_val == "sql_table" {
                return cur;
            }
            cur = self.ensure_child_of(cur, std::slice::from_ref(name));
        }
        cur
    }

    /// Connect two objects by creating an edge. Returns the edge index.
    pub fn connect(
        &mut self,
        parent: ObjId,
        src_path: &[String],
        dst_path: &[String],
        src_arrow: bool,
        dst_arrow: bool,
        label: &str,
    ) -> Result<usize, String> {
        let src = self.ensure_child_edge_of(parent, src_path);
        let dst = self.ensure_child_edge_of(parent, dst_path);

        let arrow_str = if src_arrow && dst_arrow {
            "<->"
        } else if src_arrow {
            "<-"
        } else if dst_arrow {
            "->"
        } else {
            "--"
        };
        // Match Go d2graph.Edge.initIndex: the index counts only edges with
        // the same (src, src_arrow, dst, dst_arrow) tuple — not the global
        // edge count. So a graph with edges `a -> b` then `a -> c` produces
        // `(a -> b)[0]` and `(a -> c)[0]`, not `[0]` and `[1]`.
        let index = self
            .edges
            .iter()
            .filter(|e| {
                e.src == src && e.dst == dst && e.src_arrow == src_arrow && e.dst_arrow == dst_arrow
            })
            .count();

        // Match Go `Edge.AbsID`: strip any common ancestor path shared by
        // src and dst (leaving at least the last segment on each side) and
        // emit it as a `common.` prefix outside the `(src -> dst)` block.
        // So `finally.a -> finally.tree` becomes `finally.(a -> tree)[0]`
        // instead of `(finally.a -> finally.tree)[0]`.
        let src_ida: Vec<String> = self.objects[src]
            .abs_id
            .split('.')
            .map(str::to_owned)
            .collect();
        let dst_ida: Vec<String> = self.objects[dst]
            .abs_id
            .split('.')
            .map(str::to_owned)
            .collect();
        let mut common: Vec<String> = Vec::new();
        let mut s_idx = 0usize;
        let mut d_idx = 0usize;
        while src_ida.len() - s_idx > 1 && dst_ida.len() - d_idx > 1 {
            if !src_ida[s_idx].eq_ignore_ascii_case(&dst_ida[d_idx]) {
                break;
            }
            common.push(src_ida[s_idx].clone());
            s_idx += 1;
            d_idx += 1;
        }
        let common_key = if common.is_empty() {
            String::new()
        } else {
            format!("{}.", common.join("."))
        };
        let src_tail = src_ida[s_idx..].join(".");
        let dst_tail = dst_ida[d_idx..].join(".");
        let abs_id = format!(
            "{}({} {} {})[{}]",
            common_key, src_tail, arrow_str, dst_tail, index
        );

        // Port of Go `addSQLTableColumnIndices`: when an edge endpoint
        // path walks *into* a sql_table (e.g. `a.user_id -> b.id`), we
        // want to track which column index is referenced so the
        // renderer can pin the edge at the column row.
        let parent_abs_len = if parent == self.root {
            0
        } else {
            self.objects[parent].abs_id.split('.').count()
        };
        let mut src_column_idx: Option<usize> = None;
        let mut dst_column_idx: Option<usize> = None;
        if self.objects[src].shape.value == "sql_table" && src != dst {
            let src_abs_len = self.objects[src].abs_id.split('.').count();
            if parent_abs_len + src_path.len() > src_abs_len
                && let Some(last) = src_path.last()
                && let Some(table) = self.objects[src].sql_table.as_ref()
            {
                for (i, col) in table.columns.iter().enumerate() {
                    if col.name.label == *last {
                        src_column_idx = Some(i);
                        break;
                    }
                }
            }
        }
        if self.objects[dst].shape.value == "sql_table" {
            let dst_abs_len = self.objects[dst].abs_id.split('.').count();
            if parent_abs_len + dst_path.len() > dst_abs_len
                && let Some(last) = dst_path.last()
                && let Some(table) = self.objects[dst].sql_table.as_ref()
            {
                for (i, col) in table.columns.iter().enumerate() {
                    if col.name.label == *last {
                        dst_column_idx = Some(i);
                        break;
                    }
                }
            }
        }

        let edge = Edge {
            abs_id,
            src,
            dst,
            src_arrow,
            dst_arrow,
            label: Label {
                value: label.to_string(),
                ..Default::default()
            },
            src_table_column_index: src_column_idx,
            dst_table_column_index: dst_column_idx,
            ..Default::default()
        };
        let edge_idx = self.edges.len();
        self.edges.push(edge);
        Ok(edge_idx)
    }

    /// Check if there's an object at the given id_val path from root.
    pub fn has_child(&self, path: &[String]) -> Option<ObjId> {
        let mut cur = self.root;
        for name in path {
            let found = self.objects[cur]
                .children_array
                .iter()
                .find(|&&cid| self.objects[cid].id_val().eq_ignore_ascii_case(name));
            match found {
                Some(&cid) => cur = cid,
                None => return None,
            }
        }
        Some(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_creation() {
        let g = Graph::new();
        assert_eq!(g.objects.len(), 1); // root only
        assert_eq!(g.edges.len(), 0);
        assert_eq!(g.root, 0);
    }

    #[test]
    fn add_object_and_edge() {
        let mut g = Graph::new();
        let a = g.add_object(Object {
            id: "a".into(),
            abs_id: "a".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        let b = g.add_object(Object {
            id: "b".into(),
            abs_id: "b".into(),
            width: 100.0,
            height: 50.0,
            ..Default::default()
        });
        let _e = g.add_edge(Edge {
            abs_id: "(a -> b)[0]".into(),
            src: a,
            dst: b,
            ..Default::default()
        });
        assert_eq!(g.objects.len(), 3); // root + a + b
        assert_eq!(g.edges.len(), 1);
        assert!(g.objects[a].parent == Some(0));
    }

    #[test]
    fn object_is_container() {
        let mut g = Graph::new();
        let parent = g.add_object(Object {
            id: "parent".into(),
            abs_id: "parent".into(),
            ..Default::default()
        });
        let _child = g.add_object(Object {
            id: "child".into(),
            abs_id: "parent.child".into(),
            parent: Some(parent),
            ..Default::default()
        });
        assert!(g.objects[parent].is_container());
    }

    #[test]
    fn object_get_stroke_matches_go_defaults() {
        let text = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_TEXT.into(),
            },
            ..Default::default()
        };
        assert_eq!(text.get_stroke(0.0), crate::color::N1);

        let code = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_CODE.into(),
            },
            ..Default::default()
        };
        assert_eq!(code.get_stroke(0.0), crate::color::N1);

        let class = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_CLASS.into(),
            },
            ..Default::default()
        };
        assert_eq!(class.get_stroke(0.0), crate::color::N7);

        let dashed = Object::default();
        assert_eq!(dashed.get_stroke(5.0), crate::color::B2);
    }

    #[test]
    fn edge_get_stroke_matches_go_defaults() {
        let edge = Edge::default();
        assert_eq!(edge.get_stroke(0.0), crate::color::B1);
        assert_eq!(edge.get_stroke(5.0), crate::color::B2);
    }

    #[test]
    fn has_label_and_icon_match_go_special_cases() {
        let text = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_TEXT.into(),
            },
            label: Label {
                value: "hello".into(),
                ..Default::default()
            },
            icon: Some("https://example.com/icon.svg".into()),
            ..Default::default()
        };
        assert!(!text.has_label());
        assert!(text.has_icon());

        let code = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_CODE.into(),
            },
            label: Label {
                value: "hello".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(!code.has_label());

        let image = Object {
            shape: ScalarValue {
                value: crate::target::SHAPE_IMAGE.into(),
            },
            icon: Some("https://example.com/icon.svg".into()),
            ..Default::default()
        };
        assert!(!image.has_icon());
    }
}
