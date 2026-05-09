/**
 * Generate reference layout data from dagre.js for cross-validation.
 * Runs a set of test graphs through dagre.js and saves the results as JSON.
 */
import { Graph } from '../ref/dagre-js/dist/dagre.esm.js';
import { layout } from '../ref/dagre-js/dist/dagre.esm.js';
import { writeFileSync } from 'fs';

const testCases = [];

// Helper to create a graph and run layout
function runCase(name, buildFn, opts = {}) {
    const g = new Graph({ multigraph: true, compound: true });
    g.setGraph(opts);
    g.setDefaultEdgeLabel(() => ({}));
    buildFn(g);
    layout(g);

    const nodes = {};
    g.nodes().forEach(v => {
        const n = g.node(v);
        if (n && !n.dummy) {
            nodes[v] = {
                x: Math.round(n.x * 1000) / 1000,
                y: Math.round(n.y * 1000) / 1000,
                width: n.width,
                height: n.height,
                rank: n.rank,
                order: n.order,
            };
        }
    });

    const edges = [];
    g.edges().forEach(e => {
        const label = g.edge(e);
        edges.push({
            v: e.v, w: e.w,
            points: (label.points || []).map(p => ({
                x: Math.round(p.x * 1000) / 1000,
                y: Math.round(p.y * 1000) / 1000,
            })),
            x: label.x != null ? Math.round(label.x * 1000) / 1000 : null,
            y: label.y != null ? Math.round(label.y * 1000) / 1000 : null,
        });
    });

    const graphLabel = g.graph();
    testCases.push({
        name,
        opts,
        nodes,
        edges,
        graph: {
            width: Math.round((graphLabel.width || 0) * 1000) / 1000,
            height: Math.round((graphLabel.height || 0) * 1000) / 1000,
        },
    });
}

// --- Test case 1: Single node ---
runCase("single_node", g => {
    g.setNode("a", { width: 50, height: 100 });
});

// --- Test case 2: Two connected nodes ---
runCase("two_nodes", g => {
    g.setNode("a", { width: 50, height: 100 });
    g.setNode("b", { width: 75, height: 200 });
    g.setEdge("a", "b");
});

// --- Test case 3: Diamond ---
runCase("diamond", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setNode("d", { width: 50, height: 50 });
    g.setEdge("a", "b");
    g.setEdge("a", "c");
    g.setEdge("b", "d");
    g.setEdge("c", "d");
});

// --- Test case 4: Chain of 5 ---
runCase("chain_5", g => {
    for (const v of ["a", "b", "c", "d", "e"]) {
        g.setNode(v, { width: 30, height: 20 });
    }
    g.setEdge("a", "b");
    g.setEdge("b", "c");
    g.setEdge("c", "d");
    g.setEdge("d", "e");
});

// --- Test case 5: Two nodes with edge label ---
runCase("edge_label", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b", { width: 80, height: 20, labelpos: "c" });
});

// --- Test case 6: Cycle (a->b->c->a) ---
runCase("cycle", g => {
    g.setNode("a", { width: 30, height: 20 });
    g.setNode("b", { width: 30, height: 20 });
    g.setNode("c", { width: 30, height: 20 });
    g.setEdge("a", "b");
    g.setEdge("b", "c");
    g.setEdge("c", "a");
});

// --- Test case 7: Disconnected components ---
runCase("disconnected", g => {
    g.setNode("a", { width: 30, height: 20 });
    g.setNode("b", { width: 30, height: 20 });
    g.setNode("c", { width: 30, height: 20 });
    g.setNode("d", { width: 30, height: 20 });
    g.setEdge("a", "b");
    g.setEdge("c", "d");
});

// --- Test case 8: LR direction ---
runCase("lr_direction", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b");
}, { rankdir: "LR" });

// --- Test case 9: BT direction ---
runCase("bt_direction", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b");
}, { rankdir: "BT" });

// --- Test case 10: RL direction ---
runCase("rl_direction", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b");
}, { rankdir: "RL" });

// --- Test case 11: Custom separators ---
runCase("custom_sep", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setEdge("a", "b");
    g.setEdge("a", "c");
}, { nodesep: 100, ranksep: 80, edgesep: 30 });

// --- Test case 12: Self-loop ---
runCase("self_loop", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b");
    g.setEdge("a", "a", { width: 40, height: 20 });
});

// --- Test case 13: Long edge (spans 3 ranks) ---
runCase("long_edge", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setEdge("a", "b");
    g.setEdge("a", "c");
    g.setEdge("b", "c"); // c at rank 2
});

// --- Test case 14: Wide fan-out ---
runCase("fan_out", g => {
    g.setNode("root", { width: 50, height: 50 });
    for (let i = 0; i < 5; i++) {
        const v = `n${i}`;
        g.setNode(v, { width: 40, height: 30 });
        g.setEdge("root", v);
    }
});

// --- Test case 15: Margins ---
runCase("margins", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b");
}, { marginx: 20, marginy: 30 });

// --- Test case 16: Different node sizes ---
runCase("varied_sizes", g => {
    g.setNode("small", { width: 20, height: 20 });
    g.setNode("medium", { width: 80, height: 40 });
    g.setNode("large", { width: 200, height: 100 });
    g.setEdge("small", "medium");
    g.setEdge("small", "large");
    g.setEdge("medium", "large");
});

// --- Test case 17: Parallel edges (multi-edge) ---
runCase("parallel_edges", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b", {}, "edge1");
    g.setEdge("a", "b", {}, "edge2");
});

// --- Test case 18: Complex graph ---
runCase("complex", g => {
    for (const v of ["a","b","c","d","e","f","g","h"]) {
        g.setNode(v, { width: 40, height: 30 });
    }
    g.setEdge("a","b"); g.setEdge("a","c"); g.setEdge("b","d");
    g.setEdge("b","e"); g.setEdge("c","f"); g.setEdge("c","g");
    g.setEdge("d","h"); g.setEdge("e","h"); g.setEdge("f","h");
    g.setEdge("g","h");
});

// --- Test case 19: Compound graph ---
runCase("compound", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setNode("group", {});
    g.setParent("a", "group");
    g.setParent("b", "group");
    g.setEdge("a", "c");
    g.setEdge("b", "c");
});

// --- Test case 20: Edge with minlen ---
runCase("minlen", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setEdge("a", "b", { minlen: 3 });
});

writeFileSync(
    new URL('./reference_data.json', import.meta.url),
    JSON.stringify(testCases, null, 2)
);
console.log(`Generated ${testCases.length} reference test cases.`);
