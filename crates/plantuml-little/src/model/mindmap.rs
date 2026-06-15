//! Mindmap diagram intermediate representation.

/// A single node in the mindmap tree.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct MindmapNode {
    /// Display text (may contain `\n` for line breaks).
    pub text: String,
    /// Nesting level (1 = root).
    pub level: usize,
    /// Child nodes.
    pub children: Vec<MindmapNode>,
}

/// A note annotation on the mindmap diagram.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct MindmapNote {
    pub text: String,
    pub position: String,
}

/// Top-level mindmap diagram.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "semantic-serde", derive(serde::Serialize))]
pub struct MindmapDiagram {
    /// The root node of the mindmap tree.
    pub root: MindmapNode,
    pub notes: Vec<MindmapNote>,
    /// Caption text displayed below the diagram.
    pub caption: Option<String>,
}

impl MindmapNode {
    /// Create a new node with the given text and level.
    pub fn new(text: &str, level: usize) -> Self {
        Self {
            text: text.to_string(),
            level,
            children: Vec::new(),
        }
    }

    /// Return the text split by embedded `\n` sequences.
    pub fn text_lines(&self) -> Vec<&str> {
        self.text
            .split("\\n")
            .flat_map(|s| s.split(crate::NEWLINE_CHAR))
            .map(str::trim)
            .collect()
    }

    /// Total number of descendant nodes (including self).
    pub fn descendant_count(&self) -> usize {
        1 + self
            .children
            .iter()
            .map(MindmapNode::descendant_count)
            .sum::<usize>()
    }

    /// Maximum depth from this node (1 for a leaf).
    pub fn depth(&self) -> usize {
        if self.children.is_empty() {
            1
        } else {
            1 + self
                .children
                .iter()
                .map(MindmapNode::depth)
                .max()
                .unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_new_basic() {
        let n = MindmapNode::new("hello", 1);
        assert_eq!(n.text, "hello");
        assert_eq!(n.level, 1);
        assert!(n.children.is_empty());
    }

    #[test]
    fn text_lines_single() {
        let n = MindmapNode::new("hello world", 1);
        assert_eq!(n.text_lines(), vec!["hello world"]);
    }

    #[test]
    fn text_lines_multiline() {
        let n = MindmapNode::new("They are \\ngreat and \\n should have them!", 3);
        assert_eq!(
            n.text_lines(),
            vec!["They are", "great and", "should have them!"]
        );
    }

    #[test]
    fn descendant_count_leaf() {
        let n = MindmapNode::new("leaf", 2);
        assert_eq!(n.descendant_count(), 1);
    }

    #[test]
    fn descendant_count_tree() {
        let mut root = MindmapNode::new("root", 1);
        let mut child = MindmapNode::new("child", 2);
        child.children.push(MindmapNode::new("grandchild", 3));
        root.children.push(child);
        root.children.push(MindmapNode::new("child2", 2));
        assert_eq!(root.descendant_count(), 4);
    }

    #[test]
    fn depth_leaf() {
        let n = MindmapNode::new("leaf", 1);
        assert_eq!(n.depth(), 1);
    }

    #[test]
    fn depth_nested() {
        let mut root = MindmapNode::new("root", 1);
        let mut child = MindmapNode::new("child", 2);
        child.children.push(MindmapNode::new("grandchild", 3));
        root.children.push(child);
        assert_eq!(root.depth(), 3);
    }

    #[test]
    fn diagram_struct() {
        let diagram = MindmapDiagram {
            root: MindmapNode::new("center", 1),
            notes: vec![],
            caption: None,
        };
        assert_eq!(diagram.root.text, "center");
    }

    #[test]
    fn node_clone_is_independent() {
        let mut n = MindmapNode::new("a", 1);
        let mut cloned = n.clone();
        cloned.children.push(MindmapNode::new("b", 2));
        assert!(n.children.is_empty());
        n.children.push(MindmapNode::new("c", 2));
        assert_eq!(cloned.children.len(), 1);
        assert_eq!(n.children.len(), 1);
        assert_eq!(cloned.children[0].text, "b");
        assert_eq!(n.children[0].text, "c");
    }

    #[test]
    fn text_lines_no_trailing_spaces() {
        let n = MindmapNode::new("a \\n b \\n c", 1);
        assert_eq!(n.text_lines(), vec!["a", "b", "c"]);
    }

    #[test]
    fn depth_wide_tree() {
        let mut root = MindmapNode::new("root", 1);
        for i in 0..5 {
            root.children.push(MindmapNode::new(&format!("c{}", i), 2));
        }
        assert_eq!(root.depth(), 2);
    }

    #[test]
    fn eq_trait() {
        let a = MindmapNode::new("hello", 1);
        let b = MindmapNode::new("hello", 1);
        assert_eq!(a, b);
        let c = MindmapNode::new("world", 1);
        assert_ne!(a, c);
    }
}
