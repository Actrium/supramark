// Test Promise.all order with async insertNode-like operations
const results = [];
await Promise.all(['c', 'B', 'b', 'a'].map(async (v) => {
  if (v === 'B') {
    // Non-recursive cluster path - no insertNode, just record
    results.push(`cluster:${v}`);
  } else {
    // Simulate insertNode (async)
    await Promise.resolve();
    results.push(`node:${v}`);
  }
}));
console.log('Promise.all result order:', results);
