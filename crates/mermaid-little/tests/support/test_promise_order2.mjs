// Test Promise.all order with more realistic async insertNode
const results = [];

async function insertNode(v) {
  // Simulate the actual insertNode which awaits shapeHandler
  await Promise.resolve(); // microtask
  results.push(v);
}

async function simulate() {
  await Promise.all(['c', 'B', 'b', 'a'].map(async (v) => {
    if (v === 'B') {
      // Non-recursive cluster: no insertNode, just clusterDb.set
      // This is synchronous
    } else {
      await insertNode(v);
    }
  }));
  console.log('Result order:', results);
}

simulate();
