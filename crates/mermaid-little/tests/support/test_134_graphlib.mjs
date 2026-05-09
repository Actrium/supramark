// Reproduce the upstream extract+copy+dagre flow exactly for fixture 134
import * as graphlib from '/ext/mermaid/tests/support/node_modules/dagre-d3-es/src/graphlib/index.js';
import { layout as dagreLayout } from '/ext/mermaid/tests/support/node_modules/dagre-d3-es/src/dagre/index.js';

const Graph = graphlib.Graph;

// Step 1: Build outer graph matching upstream getData()
// reversed subgraphs: B, A, O
// vertices: b, a, c (upstream getVertices order for 134)
const g = new Graph({ multigraph: true, compound: true });
g.setGraph({ rankdir: 'TB', nodesep: 50, ranksep: 50 });

// Reversed subgraphs
g.setNode('B', { id: 'B', isGroup: true });
g.setParent('B', 'A');
g.setNode('A', { id: 'A', isGroup: true });
g.setParent('A', 'O');
g.setNode('O', { id: 'O', isGroup: true });

// Vertices in upstream getVertices order
g.setNode('b', { id: 'b' });
g.setParent('b', 'A');
g.setNode('a', { id: 'a' });
g.setParent('a', 'A');
g.setNode('c', { id: 'c' });
g.setParent('c', 'B');

// Edges (after adjustClustersAndEdges retargeting)
g.setEdge('b', 'c', {}, 'L_b_B_0');
g.setEdge('a', 'c', {}, 'L_a_c_0');

console.log('Outer graph nodes:', g.nodes());
console.log('children(A):', g.children('A'));
console.log('children(B):', g.children('B'));
console.log('children(O):', g.children('O'));

// Step 2: Simulate extractor + copy for A
function copy(clusterId, graph, newGraph, rootId) {
  const nodes = graph.children(clusterId) || [];
  if (clusterId !== rootId) {
    nodes.push(clusterId);
  }
  console.log(`copy(${clusterId}, root=${rootId}): nodes = [${nodes}]`);
  
  nodes.forEach((node) => {
    if (graph.children(node).length > 0) {
      copy(node, graph, newGraph, rootId);
    } else {
      const data = graph.node(node);
      newGraph.setNode(node, data);
      if (rootId !== graph.parent(node)) {
        newGraph.setParent(node, graph.parent(node));
      }
      if (clusterId !== rootId && node !== clusterId) {
        newGraph.setParent(node, clusterId);
      }
      graph.removeNode(node);
    }
  });
}

const innerGraph = new Graph({ multigraph: true, compound: true });
innerGraph.setGraph({ rankdir: 'LR', nodesep: 50, ranksep: 50 });

copy('A', g, innerGraph, 'A');

console.log('\nInner graph nodes():', innerGraph.nodes());
console.log('Inner children(A):', innerGraph.children('A'));
console.log('Inner children(B):', innerGraph.children('B'));

// Now run dagre on inner graph
innerGraph.setNode('A', { id: 'A', isGroup: true });
dagreLayout(innerGraph);

console.log('\nAfter dagre, inner nodes():', innerGraph.nodes());
for (const n of innerGraph.nodes()) {
  const node = innerGraph.node(n);
  console.log(`  ${n}: x=${node?.x} y=${node?.y} width=${node?.width} height=${node?.height}`);
}
