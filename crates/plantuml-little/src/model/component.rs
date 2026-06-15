/// Component/Deployment diagram IR

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum ComponentKind {
    Component,
    Interface,
    Rectangle,
    Node,
    Database,
    Cloud,
    Package,
    Card,
    // Deployment diagram kinds
    Artifact,
    Storage,
    Folder,
    Frame,
    Agent,
    Archimate,
    Stack,
    Queue,
    // Port kinds (used inside component groups)
    PortIn,
    PortOut,
    // Use case diagram kinds
    Actor,
    UseCase,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ComponentEntity {
    pub name: String,
    pub id: String,
    /// Java "code": the alias if explicitly given, otherwise the display name.
    /// Used for qualified-name attributes and HTML comments.
    pub code: String,
    pub kind: ComponentKind,
    pub stereotype: Option<String>,
    /// Full list of stereotypes in declaration order. Includes the value
    /// stored in `stereotype` (the first one) plus any additional
    /// `<<tag>>` markers. Used for stereotype-keyed skinparam lookups
    /// (C4 stdlib emits chained stereotypes like `<<container>><<boundary>>`).
    pub stereotypes: Vec<String>,
    pub description: Vec<String>,
    /// Parent group name (if nested inside a rectangle/package)
    pub parent: Option<String>,
    /// Optional background color (e.g. "#FF0000" or "LightBlue")
    pub color: Option<String>,
    /// Source line number (0-based) for data-source-line attribute
    pub source_line: Option<usize>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ComponentLink {
    pub from: String,
    pub to: String,
    pub label: String,
    pub dashed: bool,
    pub direction_hint: Option<String>,
    /// Arrow stem length (dash/dot count). 1=horizontal, 2+=vertical.
    pub arrow_len: usize,
    /// Source line number (1-based) for data-source-line attribute
    pub source_line: Option<usize>,
    /// True when this link was a forward arrow with direction UP or LEFT,
    /// meaning Java would call `Link.getInv()` which consumes an extra UID.
    /// This is distinct from backward arrows like `B <-down- A` where the
    /// parser inverts direction but Java does NOT call `getInv()`.
    pub direction_inverted: bool,
    /// True when the head side of the arrow uses `>>` (Java
    /// `LinkDecor.ARROW_TRIANGLE`).  C4 stdlib `Rel(...)` expands to
    /// `-->>`, producing a hollow 4-point triangle instead of the
    /// 5-point rhombus drawn for plain `>` (`LinkDecor.ARROW`).
    pub head_arrow_triangle: bool,
    /// True when the tail side of the arrow uses `<<` (mirror of
    /// `head_arrow_triangle` for backward links).
    pub tail_arrow_triangle: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ComponentGroup {
    pub name: String,
    pub id: String,
    /// Java "code": the alias if explicitly given, otherwise the display name.
    pub code: String,
    pub kind: ComponentKind,
    pub stereotype: Option<String>,
    /// Full list of stereotypes in declaration order. Used for
    /// stereotype-keyed skinparam lookups on clusters
    /// (C4 stdlib: `<<system_boundary>><<boundary>>`).
    pub stereotypes: Vec<String>,
    pub children: Vec<String>,
    /// Source line number (1-based) for data-source-line attribute
    pub source_line: Option<usize>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ComponentDiagram {
    pub entities: Vec<ComponentEntity>,
    pub links: Vec<ComponentLink>,
    pub groups: Vec<ComponentGroup>,
    pub notes: Vec<ComponentNote>,
    pub direction: super::diagram::Direction,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct ComponentNote {
    pub text: String,
    pub position: String,
    pub target: Option<String>,
    /// Source line number (1-based) of the `note` command in the PlantUML source.
    pub source_line: Option<usize>,
    /// True when parsed as a multi-line block (`note ... / end note`).
    pub is_block: bool,
}

/// Convert a UseCaseDiagram into a ComponentDiagram so it can be routed through
/// the component (description diagram) svek/graphviz layout pipeline.
impl From<&super::usecase::UseCaseDiagram> for ComponentDiagram {
    fn from(uc: &super::usecase::UseCaseDiagram) -> Self {
        let mut entities = Vec::new();

        // Actors -> ComponentEntity with kind=Actor
        for actor in &uc.actors {
            let stereo_list: Vec<String> = actor.stereotype.iter().cloned().collect();
            entities.push(ComponentEntity {
                name: actor.name.clone(),
                id: actor.id.clone(),
                code: actor.code.clone(),
                kind: ComponentKind::Actor,
                stereotype: actor.stereotype.clone(),
                stereotypes: stereo_list,
                description: vec![],
                parent: None,
                color: actor.color.clone(),
                source_line: actor.source_line,
            });
        }

        // Use cases -> ComponentEntity with kind=UseCase
        for usecase in &uc.usecases {
            let stereo_list: Vec<String> = usecase.stereotype.iter().cloned().collect();
            entities.push(ComponentEntity {
                name: usecase.name.clone(),
                id: usecase.id.clone(),
                code: usecase.code.clone(),
                kind: ComponentKind::UseCase,
                stereotype: usecase.stereotype.clone(),
                stereotypes: stereo_list,
                description: vec![],
                parent: usecase.parent.clone(),
                color: usecase.color.clone(),
                source_line: usecase.source_line,
            });
        }

        // Links -> ComponentLink
        let links: Vec<ComponentLink> = uc
            .links
            .iter()
            .map(|link| {
                let dashed = link.style != super::usecase::UseCaseLinkStyle::Association;
                ComponentLink {
                    from: link.from.clone(),
                    to: link.to.clone(),
                    label: link.label.clone(),
                    dashed,
                    direction_hint: link.direction_hint.clone(),
                    arrow_len: 2, // default vertical layout
                    source_line: link.source_line,
                    direction_inverted: false,
                    head_arrow_triangle: false,
                    tail_arrow_triangle: false,
                }
            })
            .collect();

        // Boundaries -> ComponentGroup
        let groups: Vec<ComponentGroup> = uc
            .boundaries
            .iter()
            .map(|b| ComponentGroup {
                name: b.name.clone(),
                id: b.id.clone(),
                code: b.name.clone(),
                kind: ComponentKind::Rectangle,
                stereotype: None,
                stereotypes: Vec::new(),
                children: b.children.clone(),
                source_line: None,
            })
            .collect();

        // Notes -> ComponentNote
        let notes: Vec<ComponentNote> = uc
            .notes
            .iter()
            .map(|n| ComponentNote {
                text: n.text.clone(),
                position: n.position.clone(),
                target: n.target.clone(),
                source_line: None,
                is_block: false,
            })
            .collect();

        ComponentDiagram {
            entities,
            links,
            groups,
            notes,
            direction: uc.direction.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_kind_eq() {
        assert_eq!(ComponentKind::Component, ComponentKind::Component);
        assert_ne!(ComponentKind::Component, ComponentKind::Rectangle);
    }

    #[test]
    fn test_component_entity_creation() {
        let e = ComponentEntity {
            name: "test".to_string(),
            id: "test".to_string(),
            code: "test".to_string(),
            kind: ComponentKind::Component,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![],
            parent: None,
            color: None,
            source_line: None,
        };
        assert_eq!(e.name, "test");
        assert_eq!(e.kind, ComponentKind::Component);
    }

    #[test]
    fn test_component_link_creation() {
        let l = ComponentLink {
            from: "A".to_string(),
            to: "B".to_string(),
            label: "uses".to_string(),
            dashed: false,
            direction_hint: Some("right".to_string()),
            arrow_len: 2,
            source_line: Some(3),
            direction_inverted: false,
            head_arrow_triangle: false,
            tail_arrow_triangle: false,
        };
        assert_eq!(l.from, "A");
        assert_eq!(l.direction_hint, Some("right".to_string()));
    }

    #[test]
    fn test_component_note_creation() {
        let n = ComponentNote {
            text: "hello\nworld".to_string(),
            position: "top".to_string(),
            target: Some("comp1".to_string()),
            source_line: None,
            is_block: false,
        };
        assert_eq!(n.position, "top");
        assert!(n.target.is_some());
    }

    #[test]
    fn test_component_group_creation() {
        let g = ComponentGroup {
            name: "My Group".to_string(),
            id: "my_group".to_string(),
            code: "my_group".to_string(),
            kind: ComponentKind::Rectangle,
            stereotype: Some("$businessProcess".to_string()),
            stereotypes: vec!["$businessProcess".to_string()],
            children: vec!["src".to_string(), "tgt".to_string()],
            source_line: Some(3),
        };
        assert_eq!(g.children.len(), 2);
    }

    #[test]
    fn test_component_diagram_creation() {
        let d = ComponentDiagram {
            entities: vec![],
            links: vec![],
            groups: vec![],
            notes: vec![],

            direction: Default::default(),
        };
        assert!(d.entities.is_empty());
        assert!(d.links.is_empty());
    }

    #[test]
    fn test_entity_with_description() {
        let e = ComponentEntity {
            name: "A".to_string(),
            id: "A".to_string(),
            code: "A".to_string(),
            kind: ComponentKind::Rectangle,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec!["line 1".to_string(), "line 2".to_string()],
            parent: None,
            color: None,
            source_line: None,
        };
        assert_eq!(e.description.len(), 2);
    }

    #[test]
    fn test_entity_with_parent() {
        let e = ComponentEntity {
            name: "inner".to_string(),
            id: "inner".to_string(),
            code: "inner".to_string(),
            kind: ComponentKind::Rectangle,
            stereotype: None,
            stereotypes: Vec::new(),
            description: vec![],
            parent: Some("outer".to_string()),
            color: None,
            source_line: None,
        };
        assert_eq!(e.parent, Some("outer".to_string()));
    }

    #[test]
    fn test_component_kind_clone() {
        let k = ComponentKind::Database;
        let k2 = k.clone();
        assert_eq!(k, k2);
    }

    #[test]
    fn test_dashed_link() {
        let l = ComponentLink {
            from: "A".to_string(),
            to: "B".to_string(),
            label: String::new(),
            dashed: true,
            direction_hint: None,
            arrow_len: 2,
            source_line: None,
            direction_inverted: false,
            head_arrow_triangle: false,
            tail_arrow_triangle: false,
        };
        assert!(l.dashed);
        assert!(l.label.is_empty());
    }

    #[test]
    fn test_note_without_target() {
        let n = ComponentNote {
            text: "floating note".to_string(),
            position: "left".to_string(),
            target: None,
            source_line: None,
            is_block: false,
        };
        assert!(n.target.is_none());
    }

    #[test]
    fn test_all_component_kinds() {
        let kinds = [
            ComponentKind::Component,
            ComponentKind::Interface,
            ComponentKind::Rectangle,
            ComponentKind::Node,
            ComponentKind::Database,
            ComponentKind::Cloud,
            ComponentKind::Package,
            ComponentKind::Artifact,
            ComponentKind::Storage,
            ComponentKind::Folder,
            ComponentKind::Frame,
            ComponentKind::Agent,
            ComponentKind::Archimate,
            ComponentKind::Stack,
            ComponentKind::Queue,
            ComponentKind::PortIn,
            ComponentKind::PortOut,
        ];
        assert_eq!(kinds.len(), 17);
    }
}
