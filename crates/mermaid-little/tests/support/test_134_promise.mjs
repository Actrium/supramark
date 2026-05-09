// Simulate the Promise.all behavior with nested async
const domOrder = [];

async function insertNode(name) {
  // Simulate shapeHandler - one microtask tick
  await Promise.resolve();
  domOrder.push(name);
}

async function recursiveRender(nodes, parentCluster) {
  const results = await Promise.all(nodes.map(async (v) => {
    const name = v.name;
    
    // Set parent (sync code, before any await)
    if (parentCluster && !v.parent) {
      v.parent = parentCluster;
    }
    
    if (v.clusterNode) {
      // Cluster node - await recursive render
      const o = await recursiveRender(v.graph, name);
      domOrder.push(`cluster:${name}`);
    } else if (v.children && v.children.length > 0) {
      // Non-recursive cluster
      // No await - synchronous completion
    } else {
      // Leaf node - await insertNode
      await insertNode(name);
    }
  }));
}

// Simulate inner graph for 134's A
// graph.nodes() = [c, B, b, a] (insertion order from copy)
const innerGraphNodes = [
  { name: 'c', parent: 'B', clusterNode: false, children: [] },
  { name: 'B', parent: null, clusterNode: false, children: ['c'] },
  { name: 'b', parent: null, clusterNode: false, children: [] },
  { name: 'a', parent: null, clusterNode: false, children: [] },
];

await recursiveRender(innerGraphNodes, 'A');
console.log('DOM order:', domOrder);
