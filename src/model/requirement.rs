//! Requirement diagram AST.
//!
//! Ports upstream `packages/mermaid/src/diagrams/requirement/types.ts`
//! and the runtime state accumulated by `requirementDb.ts`. The
//! grammar allows six requirement types, free-form elements,
//! classDef/class/style directives and seven relationship verbs.
//!
//! Upstream keeps `requirements`, `elements`, `relations` and
//! `classes` as independent `Map<string, …>`; we mirror that shape so
//! the parser can translate jison productions one-for-one. Insertion
//! order matters for rendering, so we carry a parallel Vec<String> of
//! keys alongside the lookup map.

use crate::model::DiagramMeta;
use std::collections::BTreeMap;

/// Six requirement flavours recognised by the grammar. The string
/// values produced by `label()` match the human-readable labels
/// upstream writes into the title line (`<<Requirement>>`,
/// `<<Functional Requirement>>`, …), so render code doesn't need a
/// separate map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementKind {
    /// `requirement` keyword → "Requirement".
    Requirement,
    /// `functionalRequirement` → "Functional Requirement".
    Functional,
    /// `interfaceRequirement` → "Interface Requirement".
    Interface,
    /// `performanceRequirement` → "Performance Requirement".
    Performance,
    /// `physicalRequirement` → "Physical Requirement".
    Physical,
    /// `designConstraint` → "Design Constraint".
    DesignConstraint,
}

impl RequirementKind {
    /// Human-readable label upstream renders inside the `<<…>>` header.
    pub fn label(&self) -> &'static str {
        match self {
            RequirementKind::Requirement => "Requirement",
            RequirementKind::Functional => "Functional Requirement",
            RequirementKind::Interface => "Interface Requirement",
            RequirementKind::Performance => "Performance Requirement",
            RequirementKind::Physical => "Physical Requirement",
            RequirementKind::DesignConstraint => "Design Constraint",
        }
    }
}

/// Risk level (low | medium | high). Upstream renders the capitalised
/// form (`Low`, `Medium`, `High`) inside the `Risk:` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
        }
    }
}

/// Verification method (analysis | demonstration | inspection | test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMethod {
    Analysis,
    Demonstration,
    Inspection,
    Test,
}

impl VerifyMethod {
    pub fn label(&self) -> &'static str {
        match self {
            VerifyMethod::Analysis => "Analysis",
            VerifyMethod::Demonstration => "Demonstration",
            VerifyMethod::Inspection => "Inspection",
            VerifyMethod::Test => "Test",
        }
    }
}

/// One of the seven relationship verbs. Only `contains` uses the
/// start-side circle-cross marker; the other six render as dashed
/// lines with an end-side arrow marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relationship {
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
}

impl Relationship {
    pub fn keyword(&self) -> &'static str {
        match self {
            Relationship::Contains => "contains",
            Relationship::Copies => "copies",
            Relationship::Derives => "derives",
            Relationship::Satisfies => "satisfies",
            Relationship::Verifies => "verifies",
            Relationship::Refines => "refines",
            Relationship::Traces => "traces",
        }
    }
}

/// A parsed requirement block.
#[derive(Debug, Clone, Default)]
pub struct Requirement {
    pub name: String,
    pub kind: Option<RequirementKind>,
    pub id: String,
    pub text: String,
    pub risk: Option<RiskLevel>,
    pub verify: Option<VerifyMethod>,
    pub css_styles: Vec<String>,
    pub classes: Vec<String>,
}

/// A parsed element block (design artefact or test harness).
#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub element_type: String,
    pub doc_ref: String,
    pub css_styles: Vec<String>,
    pub classes: Vec<String>,
}

/// A single relationship statement.
#[derive(Debug, Clone)]
pub struct Relation {
    pub kind: Relationship,
    pub src: String,
    pub dst: String,
}

/// A user-defined style class (`classDef foo stroke:red,fill:blue`).
#[derive(Debug, Clone, Default)]
pub struct ClassDef {
    pub id: String,
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

/// The fully parsed requirement diagram.
#[derive(Debug, Clone, Default)]
pub struct RequirementDiagram {
    pub meta: DiagramMeta,
    /// `TB | BT | LR | RL` — upstream default is `TB`.
    pub direction: String,
    /// Requirement insertion order (parallel to `requirements_map`).
    pub requirement_order: Vec<String>,
    /// Requirement blocks keyed by name.
    pub requirements_map: BTreeMap<String, Requirement>,
    /// Element insertion order.
    pub element_order: Vec<String>,
    /// Element blocks keyed by name.
    pub elements_map: BTreeMap<String, Element>,
    /// Relationships in source order.
    pub relations: Vec<Relation>,
    /// `classDef` registrations, in insertion order.
    pub class_def_order: Vec<String>,
    pub class_defs: BTreeMap<String, ClassDef>,
}

impl RequirementDiagram {
    pub fn new() -> Self {
        Self {
            direction: "TB".into(),
            ..Self::default()
        }
    }

    /// Insertion-ordered iterator over requirements.
    pub fn requirements(&self) -> impl Iterator<Item = &Requirement> {
        self.requirement_order
            .iter()
            .filter_map(move |k| self.requirements_map.get(k))
    }

    /// Insertion-ordered iterator over elements.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.element_order
            .iter()
            .filter_map(move |k| self.elements_map.get(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_labels_match_upstream() {
        assert_eq!(RequirementKind::Requirement.label(), "Requirement");
        assert_eq!(
            RequirementKind::Functional.label(),
            "Functional Requirement"
        );
        assert_eq!(
            RequirementKind::DesignConstraint.label(),
            "Design Constraint"
        );
    }

    #[test]
    fn relationship_keywords_are_stable() {
        assert_eq!(Relationship::Contains.keyword(), "contains");
        assert_eq!(Relationship::Traces.keyword(), "traces");
    }

    #[test]
    fn default_direction_is_tb() {
        let d = RequirementDiagram::new();
        assert_eq!(d.direction, "TB");
    }
}
