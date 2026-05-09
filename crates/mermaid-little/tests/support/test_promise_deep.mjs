const domOrder = [];

// Simulate shapeHandler - async function with no internal await (like squareRect)
// But shapeHandler itself creates a Promise wrapper
async function shapeHandler(name) {
  return name; // sync return, but wrapped in Promise
}

// Simulate insertNode - has one await for shapeHandler
async function insertNode(name) {
  const el = await shapeHandler(name);
  domOrder.push(name);
  return el;
}

// Simulate recursiveRender's Promise.all
async function recursiveRender(nodes, parentCluster) {
  await Promise.all(nodes.map(async (v) => {
    const { name, parent, isClusterNode, hasChildren } = v;
    
    // Sync: setParent
    if (parentCluster && !parent) {
      v._parent = parentCluster;
    }
    
    if (isClusterNode) {
      await recursiveRender(v.graph || [], name);
    } else if (hasChildren) {
      // non-recursive cluster: sync only, no await
    } else {
      await insertNode(name);
    }
  }));
}

// 134 inner: [c, B, b, a]
const nodes134 = [
  { name: 'c', parent: 'B', isClusterNode: false, hasChildren: false },
  { name: 'B', parent: null, isClusterNode: false, hasChildren: true },
  { name: 'b', parent: null, isClusterNode: false, hasChildren: false },
  { name: 'a', parent: null, isClusterNode: false, hasChildren: false },
];

await recursiveRender(nodes134, 'A');
console.log('Deep nested DOM order:', domOrder);
