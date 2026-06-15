//! Entity-Relationship diagram model.
//!
//! Ports upstream `packages/mermaid/src/diagrams/er/erTypes.ts` and the
//! `ErDB` storage shape from `erDb.ts`. Every optional TS field maps to
//! `Option<_>`; arrays to `Vec<_>`.
//!
//! ER syntax summary (cross-ref `parser/erDiagram.jison`):
//!
//! * `erDiagram` header
//! * `ENTITY_NAME [alias?] { attributes? }` — declare entity; attributes
//!   are `type name [keys] [comment]` rows.
//! * `ENTITY rel ENTITY : role` — a relationship between two entities.
//!   Cardinality tokens: `||` (only-one), `|o` (zero-or-one), `}o`
//!   (zero-or-more), `}|` (one-or-more), `u` (markdown parent).
//! * Relationship type: `--` (identifying / solid), `..` (non-identifying / dashed),
//!   `.-` / `-.` (mixed variants, still dashed in 11.14.0).
//! * `direction TB|BT|LR|RL` — layout direction (dagre rankdir).
//! * `classDef`, `class`, `style` — CSS class / style attachments.
//!
//! The diagram preserves insertion order of entities (JS `Map` semantics)
//! via an explicit `Vec<String>` of keys alongside the lookup map.

use std::collections::BTreeMap;

use crate::model::DiagramMeta;

/// Cardinality of one endpoint of an ER relationship. Matches the
/// JS `Cardinality` enum strings verbatim for faithful error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum Cardinality {
    ZeroOrOne,
    ZeroOrMore,
    OneOrMore,
    OnlyOne,
    /// Markdown parent (`u` token) — custom mermaid extension.
    MdParent,
}

impl Cardinality {
    /// Upstream uppercase string form — used by marker-name lookups.
    pub fn as_upper(&self) -> &'static str {
        match self {
            Cardinality::ZeroOrOne => "ZERO_OR_ONE",
            Cardinality::ZeroOrMore => "ZERO_OR_MORE",
            Cardinality::OneOrMore => "ONE_OR_MORE",
            Cardinality::OnlyOne => "ONLY_ONE",
            Cardinality::MdParent => "MD_PARENT",
        }
    }

    /// Lowercase form — as populated into `Edge::arrowTypeEnd/Start`
    /// in `erDb.ts::getData` (via `.toLowerCase()`).
    pub fn as_lower(&self) -> String {
        self.as_upper().to_ascii_lowercase()
    }
}

/// Relationship identification kind. Solid vs dashed in the rendered
/// edge — `IDENTIFYING` is the solid line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum Identification {
    NonIdentifying,
    Identifying,
}

impl Identification {
    pub fn as_upper(&self) -> &'static str {
        match self {
            Identification::NonIdentifying => "NON_IDENTIFYING",
            Identification::Identifying => "IDENTIFYING",
        }
    }

    /// Edge `pattern` field — "solid" for identifying, "dashed" otherwise.
    pub fn edge_pattern(&self) -> &'static str {
        match self {
            Identification::Identifying => "solid",
            Identification::NonIdentifying => "dashed",
        }
    }
}

/// An entity attribute row — `type name [keys] [comment]`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Attribute {
    pub attr_type: String,
    pub name: String,
    /// Parsed key tokens: `PK`, `FK`, `UK`.
    pub keys: Vec<String>,
    pub comment: String,
}

/// A parsed ER entity. Order within `ErDiagram::entity_keys` matches
/// insertion order (upstream's `Map` iteration).
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Entity {
    /// The stable id generated from the entity name + insertion index,
    /// e.g. `entity-CUSTOMER-0`.
    pub id: String,
    /// The original entity name (pre-alias if any).
    pub label: String,
    /// Optional alias — `NAME["Display Name"]` syntax. Empty string when
    /// absent (matches upstream's default).
    pub alias: String,
    /// Attribute rows in original declaration order.
    pub attributes: Vec<Attribute>,
    /// Extra style strings applied via `style` statements.
    pub css_styles: Vec<String>,
    /// Whitespace-separated class list (always includes `default`).
    pub css_classes: String,
}

/// One relationship edge. `entity_a`/`entity_b` are the *entity ids*
/// (not names — upstream maps names to ids during `addRelationship`).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct Relationship {
    pub entity_a: String,
    pub role_a: String,
    pub entity_b: String,
    pub card_a: Cardinality,
    pub card_b: Cardinality,
    pub rel_type: Identification,
}

/// classDef / class payload — style lists keyed by class id.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct EntityClass {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// Parsed ER diagram.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ErDiagram {
    pub meta: DiagramMeta,
    /// Insertion order of entity names.
    pub entity_keys: Vec<String>,
    /// name → entity map.
    pub entities: BTreeMap<String, Entity>,
    pub relationships: Vec<Relationship>,
    pub classes: BTreeMap<String, EntityClass>,
    /// Layout direction token — "TB" | "BT" | "LR" | "RL". Defaults to `TB`.
    pub direction: String,
    /// Optional theme-name override sourced from frontmatter / directive.
    pub theme_override: Option<String>,
}

impl ErDiagram {
    /// Helper — add an entity if not already present; update alias in
    /// place if we learn one later. Mirrors `ErDB::addEntity`.
    pub fn add_entity(&mut self, name: &str, alias: &str) -> &mut Entity {
        if !self.entities.contains_key(name) {
            let id = format!("entity-{}-{}", name, self.entity_keys.len());
            self.entity_keys.push(name.to_string());
            self.entities.insert(
                name.to_string(),
                Entity {
                    id,
                    label: name.to_string(),
                    alias: alias.to_string(),
                    attributes: Vec::new(),
                    css_styles: Vec::new(),
                    css_classes: "default".to_string(),
                },
            );
        } else if !alias.is_empty() {
            let e = self.entities.get_mut(name).unwrap();
            if e.alias.is_empty() {
                e.alias = alias.to_string();
            }
        }
        self.entities.get_mut(name).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_entity_is_idempotent_preserves_order() {
        let mut d = ErDiagram::default();
        d.add_entity("A", "");
        d.add_entity("B", "");
        d.add_entity("A", "");
        assert_eq!(d.entity_keys, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(d.entities.get("A").unwrap().id, "entity-A-0");
        assert_eq!(d.entities.get("B").unwrap().id, "entity-B-1");
    }

    #[test]
    fn add_entity_learns_alias_later() {
        let mut d = ErDiagram::default();
        d.add_entity("A", "");
        d.add_entity("A", "Alpha");
        assert_eq!(d.entities.get("A").unwrap().alias, "Alpha");
    }

    #[test]
    fn cardinality_round_trips_lower_upper() {
        assert_eq!(Cardinality::OneOrMore.as_upper(), "ONE_OR_MORE");
        assert_eq!(Cardinality::OneOrMore.as_lower(), "one_or_more");
    }
}
