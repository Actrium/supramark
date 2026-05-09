import * as graphlib from 'dagre-d3-es/src/graphlib/index.js';

const Graph = graphlib.Graph;

// Build the outer graph like the upstream render function does
const graph = new Graph({
  multigraph: true,
  compound: true,
}).setGraph({
  rankdir: 'TB',
  nodesep: 50,
  ranksep: 50,
  marginx: 8,
  marginy: 8,
}).setDefaultEdgeLabel(function() { return {}; });

// getData order: subgraphs reversed (A, B, O), then vertices (b, a, c)
const nodes = [
  { id: 'A', isGroup: true, parentId: 'O' },
  { id: 'B', isGroup: true, parentId: 'A' },
  { id: 'O', isGroup: true, parentId: undefined },
  { id: 'b', isGroup: false, parentId: 'A' },
  { id: 'a', isGroup: false, parentId: 'A' },
  { id: 'c', isGroup: false, parentId: 'B' },
];

for (const node of nodes) {
  graph.setNode(node.id, { ...node });
  if (node.parentId) {
    graph.setParent(node.id, node.parentId);
  }
}

graph.setEdge('b', 'c', { id: 'L_b_B_0' }, 'L_b_B_0');
graph.setEdge('a', 'c', { id: 'L_a_c_0' }, 'L_a_c_0');

console.log('Outer graph nodes:', graph.nodes());
console.log('Outer graph children(A):', graph.children('A'));
console.log('Outer graph children(B):', graph.children('B'));
console.log('Outer graph children(O):', graph.children('O'));

// Check external connections
const clusterDb = new Map();
const descendantsMap = new Map();

function extractDescendants(id, g) {
  const children = g.children(id);
  let res = [...children];
  for (const child of children) {
    res = [...res, ...extractDescendants(child, g)];
  }
  return res;
}

function findNonClusterChild(id, g) {
  const children = g.children(id);
  if (children.length < 1) return id;
  let reserve;
  for (const child of children) {
    const _id = findNonClusterChild(child, g);
    if (_id) reserve = _id;
  }
  return reserve;
}

graph.nodes().forEach(function(id) {
  const children = graph.children(id);
  if (children.length > 0) {
    descendantsMap.set(id, extractDescendants(id, graph));
    clusterDb.set(id, { id: findNonClusterChild(id, graph), clusterData: graph.node(id) });
  }
});

function isDescendant(id, ancestorId) {
  const d = descendantsMap.get(ancestorId) || [];
  return d.includes(id);
}

graph.nodes().forEach(function(id) {
  const children = graph.children(id);
  if (children.length > 0) {
    graph.edges().forEach((edge) => {
      const d1 = isDescendant(edge.v, id);
      const d2 = isDescendant(edge.w, id);
      if (d1 ^ d2) {
        clusterDb.get(id).externalConnections = true;
      }
    });
  }
});

console.log('\nCluster info:');
for (const [id, info] of clusterDb) {
  console.log(`  ${id}: externalConnections=${!!info.externalConnections}, descendants=${descendantsMap.get(id)}`);
}

// Copy function (from upstream)
function copy(clusterId, g, newGraph, rootId) {
  let nodes = g.children(clusterId) || [];
  if (clusterId !== rootId) {
    nodes.push(clusterId);
  }
  console.log(`  copy(${clusterId}, root=${rootId}): processing [${nodes}]`);
  
  for (const node of nodes) {
    if (g.children(node).length > 0) {
      copy(node, g, newGraph, rootId);
    } else {
      const data = g.node(node);
      newGraph.setNode(node, { ...data });
      if (rootId !== g.parent(node)) {
        newGraph.setParent(node, g.parent(node));
      }
      if (clusterId !== rootId && node !== clusterId) {
        newGraph.setParent(node, clusterId);
      }
    }
    g.removeNode(node);
  }
}

// Extract clusters without external connections
console.log('\nExtracting clusters:');
for (const [id, info] of clusterDb) {
  if (!info.externalConnections && graph.children(id) && graph.children(id).length > 0) {
    console.log(`  Extracting cluster: ${id}`);
    const clusterGraph = new Graph({
      multigraph: true,
      compound: true,
    }).setGraph({
      rankdir: 'TB',
      nodesep: 50,
      ranksep: 50,
      marginx: 8,
      marginy: 8,
    }).setDefaultEdgeLabel(function() { return {}; });
    
    copy(id, graph, clusterGraph, id);
    
    console.log(`  Inner graph for ${id}:`);
    console.log(`    nodes():`, clusterGraph.nodes());
    for (const n of clusterGraph.nodes()) {
      console.log(`    ${n}: parent=${clusterGraph.parent(n)}, children=${JSON.stringify(clusterGraph.children(n))}`);
    }
    
    graph.setNode(id, { clusterNode: true, id: id, graph: clusterGraph });
    console.log(`  Outer graph after extraction:`, graph.nodes());
  }
}

// Also check O's inner graph
for (const [id, info] of clusterDb) {
  if (!info.externalConnections && graph.children(id) && graph.children(id).length > 0 && id !== 'A') {
    console.log(`\n  Also extracting cluster: ${id}`);
  }
}
