const domOrder = [];

// Simulate async shapeHandler (no await inside - just returns sync)
async function shapeHandler(name) {
  // No await - pure sync, but async function returns Promise
  return name;
}

// Simulate insertNode
async function insertNode(name) {
  await shapeHandler(name);
  domOrder.push(name);
}

// Simulate recursiveRender
async function recursiveRender(nodes, parentCluster) {
  await Promise.all(nodes.map(async (v) => {
    const { name, parent, isClusterNode, hasChildren } = v;
    
    // Sync code: set parent
    if (parentCluster && !parent) {
      v.parent = parentCluster;
    }
    
    if (isClusterNode) {
      // Cluster node - await recursive render
      await recursiveRender(v.graph, name);
      domOrder.push(`cluster:${name}`);
    } else if (hasChildren) {
      // Non-recursive cluster - sync only
    } else {
      await insertNode(name);
    }
  }));
}

// Test with 134 inner graph nodes order: [c, B, b, a]
const nodes134 = [
  { name: 'c', parent: 'B', isClusterNode: false, hasChildren: false },
  { name: 'B', parent: null, isClusterNode: false, hasChildren: true },
  { name: 'b', parent: null, isClusterNode: false, hasChildren: false },
  { name: 'a', parent: null, isClusterNode: false, hasChildren: false },
];

await recursiveRender(nodes134, 'A');
console.log('134 inner DOM order:', domOrder);
