/// Entity kind
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum EntityKind {
    #[default]
    Class,
    Interface,
    Enum,
    Abstract,
    Annotation,
    Object,
    Map,
    Rectangle,
    /// Component entity (rendered with component icon tabs)
    Component,
}

/// Rectangle-family symbol variant — Java USymbol sub-type for entities that
/// all map to `EntityKind::Rectangle` but need distinct rendered shapes.
/// See Java `USymbols`/`USymbolFile`/`USymbolFolder` etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum RectSymbol {
    #[default]
    Rectangle,
    File,
    Folder,
    Frame,
    Card,
    Agent,
    Storage,
    Artifact,
    Node,
    Cloud,
    Stack,
    Queue,
}

/// Member visibility
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum Visibility {
    Public,    // +
    Private,   // -
    Protected, // #
    Package,   // ~
}

/// Member modifiers
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct MemberModifiers {
    pub is_static: bool,
    pub is_abstract: bool,
}

/// Class member (field or method)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Member {
    pub visibility: Option<Visibility>,
    pub name: String,
    pub return_type: Option<String>,
    pub is_method: bool,
    pub modifiers: MemberModifiers,
    /// Raw display text (after removing visibility/modifiers), matching Java MemberImpl.getDisplay().
    /// When set, rendering uses this instead of reconstructing from name + return_type.
    pub display: Option<String>,
}

/// Stereotype (e.g. `<<Entity>>`)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Stereotype(pub String);

/// Spot extracted from a stereotype, e.g. `(E,White)` in `<< (E,White) Extension >>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct StereotypeSpot {
    pub character: char,
    pub color: Option<String>,
}

impl Stereotype {
    /// Extract a spot `(Char,Color)` or `(Char)` from the stereotype label.
    /// Returns the spot info and the cleaned label (without the spot notation).
    /// Java: `StereotypeDecoration.buildComplex()` with `circleChar` regex.
    pub fn extract_spot(&self) -> (Option<StereotypeSpot>, String) {
        let s = self.0.trim();
        // Look for pattern like (X,Color) or (X) at the start of the stereotype text
        if !s.starts_with('(') {
            return (None, self.0.clone());
        }
        if let Some(close) = s.find(')') {
            let inside = &s[1..close];
            let rest = s[close + 1..].trim().to_string();
            let parts: Vec<&str> = inside.splitn(2, ',').collect();
            let ch_str = parts[0].trim();
            if ch_str.len() == 1 {
                let ch = ch_str.chars().next().unwrap();
                let color = if parts.len() > 1 {
                    let c = parts[1].trim();
                    if !c.is_empty() {
                        Some(c.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                return (
                    Some(StereotypeSpot {
                        character: ch,
                        color,
                    }),
                    rest,
                );
            }
        }
        (None, self.0.clone())
    }
}

/// Entity (class, interface, enum, etc.)
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Entity {
    pub uid: Option<String>,
    pub name: String,
    pub kind: EntityKind,
    pub stereotypes: Vec<Stereotype>,
    pub members: Vec<Member>,
    /// Bracket-body description lines for rectangle entities (Java: `[text]`)
    pub description: Vec<String>,
    pub color: Option<String>,
    pub generic: Option<String>,
    pub source_line: Option<usize>,
    /// Entity-level visibility modifier (e.g. `-class foo` -> Private)
    pub visibility: Option<Visibility>,
    /// Display name (when `as Alias` is used, this holds the quoted label).
    pub display_name: Option<String>,
    pub map_entries: Vec<(String, String)>,
    /// Rectangle-family symbol variant (file / folder / card / …).
    /// Only meaningful when `kind == EntityKind::Rectangle`; ignored otherwise.
    pub rect_symbol: RectSymbol,
}
