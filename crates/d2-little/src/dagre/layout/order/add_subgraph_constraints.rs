//! Add subgraph ordering constraints.
//!
//! After sorting a layer, this function walks each node's parent chain
//! and adds constraint edges between sibling subgraphs to preserve the
//! ordering discovered during the current sweep.

use std::collections::HashMap;

use crate::dagre::graph::Graph;
use crate::dagre::layout::types::{EdgeLabel, NodeLabel};

/// Walk the sorted node list and add constraint edges between subgraphs
/// that appear in the same parent.
///
/// For each node in `vs`, we walk up the compound hierarchy. At each level,
/// if we've previously seen a different child of the same parent, we add
/// a constraint edge from the previous child to the current child.
pub(crate) fn add_subgraph_constraints(
    g: &Graph<NodeLabel, EdgeLabel>,
    cg: &mut Graph<(), ()>,
    vs: &[String],
) {
    let mut prev: HashMap<String, String> = HashMap::new();
    let mut root_prev: Option<String> = None;

    for v in vs {
        let mut child: Option<String> = g.parent(v).map(|s| s.to_string());
        let mut prev_child: Option<String>;

        while let Some(ref c) = child {
            let parent = g.parent(c).map(|s| s.to_string());
            if let Some(ref p) = parent {
                prev_child = prev.get(p).cloned();
                prev.insert(p.clone(), c.clone());
            } else {
                prev_child = root_prev.clone();
                root_prev = Some(c.clone());
            }

            if let Some(ref pc) = prev_child
                && pc != c
            {
                cg.set_edge(pc.clone(), c.clone(), None, None);
                break;
            }

            child = parent;
        }
    }
}
