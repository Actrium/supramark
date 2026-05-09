import * as graphlib from 'dagre-d3-es/src/graphlib/index.js';
const Graph = graphlib.Graph;

// Build inner graph for A (as determined by the copy function)
const innerGraph = new Graph({
  multigraph: true,
  compound: true,
}).setGraph({ rankdir: 'TB', nodesep: 50, ranksep: 50, marginx: 8, marginy: 8 })
  .setDefaultEdgeLabel(() => {});

// Nodes in copy order: c, B, b, a
innerGraph.setNode('c', { id: 'c', isGroup: false });
innerGraph.setParent('c', 'B');
innerGraph.setNode('B', { id: 'B', isGroup: true });
// B has no parent (it's top-level)
innerGraph.setNode('b', { id: 'b', isGroup: false });
// b has no parent
innerGraph.setNode('a', { id: 'a', isGroup: false });
// a has no parent

// Add edges
innerGraph.setEdge('b', 'c', {}, 'L_b_B_0');
innerGraph.setEdge('a', 'c', {}, 'L_a_c_0');

console.log('Initial inner graph nodes():', innerGraph.nodes());
console.log('Initial children():', innerGraph.children());
for (const n of innerGraph.nodes()) {
  console.log(`  ${n}: parent=${innerGraph.parent(n)}, children=${JSON.stringify(innerGraph.children(n))}`);
}

// Simulate recursiveRender's parentCluster step
// parentCluster.id = 'A'
const parentClusterId = 'A';

// Promise.all(graph.nodes().map(async(v) => ...))
// This runs all callbacks concurrently
const domOrder = [];

await Promise.all(innerGraph.nodes().map(async (v) => {
  const node = innerGraph.node(v);
  
  // Set parent for nodes without parent
  if (!innerGraph.parent(v)) {
    innerGraph.setParent(v, parentClusterId);
  }
  
  if (node?.clusterNode) {
    // skip for our test
  } else {
    if (innerGraph.children(v).length > 0) {
      // non-recursive cluster
    } else {
      // Regular node - simulate insertNode
      await Promise.resolve(); // simulate async
      domOrder.push(v);
    }
  }
}));

console.log('\nAfter recursiveRender parent assignments:');
console.log('inner graph nodes():', innerGraph.nodes());
console.log('children():', innerGraph.children());
for (const n of innerGraph.nodes()) {
  console.log(`  ${n}: parent=${innerGraph.parent(n)}, children=${JSON.stringify(innerGraph.children(n))}`);
}

console.log('\nDOM insertion order:', domOrder);

// Now check sortNodesByHierarchy
function sorter(g, nodes) {
  if (nodes.length === 0) return [];
  let result = [...nodes];
  for (const node of nodes) {
    const children = g.children(node);
    result = [...result, ...sorter(g, children)];
  }
  return result;
}

const hierarchyOrder = sorter(innerGraph, innerGraph.children());
console.log('sortNodesByHierarchy order:', hierarchyOrder);
