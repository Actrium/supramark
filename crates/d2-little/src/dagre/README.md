# dagre-rs

A complete Rust port of [dagre.js](https://github.com/dagrejs/dagre) -- hierarchical directed graph layout using the Sugiyama method.

## Features

- Full 27-step layout pipeline matching dagre.js behavior
- All four rank directions: TB (top-to-bottom), BT, LR, RL
- Compound (nested) graph support with subgraph boundaries
- Edge label placement and self-loop routing
- Network simplex rank assignment
- Barycenter-based crossing minimization
- Brandes-Koepf coordinate assignment (4-direction sweep + median balance)
- Graph data structure with directed/undirected, multigraph, and compound modes
- Graph algorithms: topological sort, Dijkstra, Bellman-Ford, Floyd-Warshall, Tarjan SCC, Prim MST, and more
- JSON serialization via serde
- Zero runtime dependencies (only `log` and optional `serde`)

## Compatibility

Cross-validated against dagre.js on 20 reference graphs covering single nodes, chains, diamonds, cycles, fan-outs, disconnected components, edge labels, self-loops, compound subgraphs, all four rank directions, custom separators, margins, parallel edges, and varying node sizes. All produce identical coordinates, ranks, orders, and graph dimensions.

## Usage

```rust
use dagre::graph::{Graph, GraphOptions};
use dagre::{layout, LayoutOptions, NodeLabel, EdgeLabel};

// Create a directed graph
let mut g = Graph::with_options(GraphOptions {
    directed: true,
    multigraph: true,
    compound: true,
});

// Add nodes with dimensions
let mut a = NodeLabel::default();
a.width = 50.0;
a.height = 50.0;
g.set_node("a".into(), Some(a));

let mut b = NodeLabel::default();
b.width = 50.0;
b.height = 50.0;
g.set_node("b".into(), Some(b));

// Add an edge
g.set_edge("a", "b", Some(EdgeLabel::default()), None);

// Run layout
layout(&mut g, None);

// Read results
let node_a = g.node("a").unwrap();
println!("a: x={}, y={}", node_a.x.unwrap(), node_a.y.unwrap());
```

## Configuration

```rust
use dagre::{LayoutOptions, RankDir, Ranker};

let opts = LayoutOptions {
    rankdir: RankDir::LR,     // left-to-right layout
    nodesep: 60.0,            // horizontal separation between nodes
    ranksep: 80.0,            // vertical separation between ranks
    edgesep: 20.0,            // separation between edges
    marginx: 10.0,            // horizontal margin
    marginy: 10.0,            // vertical margin
    ranker: Ranker::NetworkSimplex,
    ..Default::default()
};

layout(&mut g, Some(opts));
```

## Project stats

| Metric | Value |
|--------|-------|
| Source code | 8,500 lines |
| Test code | 7,900 lines |
| Test functions | 528 |
| Test pass rate | 100% (0 ignored, 0 failures) |
| Cross-validation | 20/20 match with dagre.js |
| Clippy warnings | 0 |
| Runtime dependencies | 0 |

## License

MIT. This module is a Rust port of [dagre.js](https://github.com/dagrejs/dagre)
(Copyright (c) 2012-2014 Chris Pettitt, MIT License). See `LICENSE` for
the full text.
