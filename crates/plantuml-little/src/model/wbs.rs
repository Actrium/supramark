/// WBS (Work Breakdown Structure) diagram IR

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub enum WbsDirection {
    #[default]
    Default,
    Right,
    Left,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WbsNode {
    pub text: String,
    pub children: Vec<WbsNode>,
    pub direction: WbsDirection,
    pub alias: Option<String>,
    pub level: usize,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WbsLink {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WbsNote {
    pub text: String,
    pub position: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct WbsDiagram {
    pub root: WbsNode,
    pub links: Vec<WbsLink>,
    pub notes: Vec<WbsNote>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_direction() {
        assert_eq!(WbsDirection::Default, WbsDirection::default());
    }

    #[test]
    fn test_wbs_node_creation() {
        let node = WbsNode {
            text: "Root".to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        assert_eq!(node.text, "Root");
        assert_eq!(node.level, 1);
        assert!(node.children.is_empty());
        assert!(node.alias.is_none());
    }

    #[test]
    fn test_wbs_node_with_alias() {
        let node = WbsNode {
            text: "Team A".to_string(),
            children: vec![],
            direction: WbsDirection::Right,
            alias: Some("TLA".to_string()),
            level: 2,
        };
        assert_eq!(node.alias.as_deref(), Some("TLA"));
        assert_eq!(node.direction, WbsDirection::Right);
    }

    #[test]
    fn test_wbs_node_with_children() {
        let child = WbsNode {
            text: "Child".to_string(),
            children: vec![],
            direction: WbsDirection::Left,
            alias: None,
            level: 2,
        };
        let root = WbsNode {
            text: "Root".to_string(),
            children: vec![child],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].text, "Child");
    }

    #[test]
    fn test_wbs_link() {
        let link = WbsLink {
            from: "TLA".to_string(),
            to: "TLB".to_string(),
        };
        assert_eq!(link.from, "TLA");
        assert_eq!(link.to, "TLB");
    }

    #[test]
    fn test_wbs_diagram() {
        let root = WbsNode {
            text: "Root".to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        let diagram = WbsDiagram {
            root,
            links: vec![],

            notes: vec![],
        };
        assert_eq!(diagram.root.text, "Root");
        assert!(diagram.links.is_empty());
    }

    #[test]
    fn test_direction_variants() {
        let d1 = WbsDirection::Default;
        let d2 = WbsDirection::Right;
        let d3 = WbsDirection::Left;
        assert_eq!(d1, WbsDirection::Default);
        assert_eq!(d2, WbsDirection::Right);
        assert_eq!(d3, WbsDirection::Left);
        assert_ne!(d1, d2);
        assert_ne!(d2, d3);
    }

    #[test]
    fn test_wbs_node_clone() {
        let node = WbsNode {
            text: "Test".to_string(),
            children: vec![WbsNode {
                text: "Inner".to_string(),
                children: vec![],
                direction: WbsDirection::Right,
                alias: Some("I".to_string()),
                level: 2,
            }],
            direction: WbsDirection::Default,
            alias: Some("T".to_string()),
            level: 1,
        };
        let cloned = node.clone();
        assert_eq!(cloned.text, node.text);
        assert_eq!(cloned.children.len(), 1);
        assert_eq!(cloned.alias, node.alias);
    }

    #[test]
    fn test_deeply_nested_tree() {
        let leaf = WbsNode {
            text: "Leaf".to_string(),
            children: vec![],
            direction: WbsDirection::Left,
            alias: None,
            level: 4,
        };
        let l3 = WbsNode {
            text: "L3".to_string(),
            children: vec![leaf],
            direction: WbsDirection::Right,
            alias: None,
            level: 3,
        };
        let l2 = WbsNode {
            text: "L2".to_string(),
            children: vec![l3],
            direction: WbsDirection::Default,
            alias: None,
            level: 2,
        };
        let root = WbsNode {
            text: "Root".to_string(),
            children: vec![l2],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        assert_eq!(root.children[0].children[0].children[0].text, "Leaf");
        assert_eq!(root.children[0].children[0].children[0].level, 4);
    }

    #[test]
    fn test_diagram_with_links() {
        let root = WbsNode {
            text: "Root".to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: Some("R".to_string()),
            level: 1,
        };
        let diagram = WbsDiagram {
            root,
            links: vec![
                WbsLink {
                    from: "A".to_string(),
                    to: "B".to_string(),
                },
                WbsLink {
                    from: "C".to_string(),
                    to: "D".to_string(),
                },
            ],
            notes: vec![],
        };
        assert_eq!(diagram.links.len(), 2);
        assert_eq!(diagram.links[0].from, "A");
        assert_eq!(diagram.links[1].to, "D");
    }

    #[test]
    fn test_multiline_text() {
        let node = WbsNode {
            text: "Line 1\nLine 2".to_string(),
            children: vec![],
            direction: WbsDirection::Default,
            alias: None,
            level: 1,
        };
        assert!(node.text.contains('\n'));
        assert_eq!(node.text.lines().count(), 2);
    }
}
