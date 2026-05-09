/**
 * Generate reference layout data from dagre.js for cross-validation.
 * Runs a set of test graphs through dagre.js and saves the results as JSON.
 *
 * The output JSON has the shape `{ "_meta": {...}, "cases": [...] }`. The
 * `_meta` block records which dagre.js version/commit produced the baseline,
 * so reviewers can audit basleine drift via `git diff`. See ../ref/README.md
 * (or cross-validate/SETUP.md) for how to set up `ref/dagre-js`.
 */
import { Graph } from '../ref/dagre-js/dist/dagre.esm.js';
import { layout } from '../ref/dagre-js/dist/dagre.esm.js';
import { readFileSync, writeFileSync } from 'fs';
import { execSync } from 'child_process';

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

// ----------------------------------------------------------------------
// Compound bbox matrix (cases 21-30).
//
// mermaid-little observed systematic 2-5 px deltas in compound (cluster)
// node sizing — see docs/dagre_rs_inconsistency_report.zh.md items
// ④⑤⑧⑨. These cases isolate the variables (rankdir, child count,
// nesting depth, cross-cluster edges) so cross-validation surfaces
// whether the bias originates in dagre-rs's port or in @dagrejs/dagre.
// ----------------------------------------------------------------------

// 21. Compound, single leaf child, TB direction.
runCase("compound_single_leaf_tb", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("g", {});
    g.setParent("a", "g");
});

// 22. Compound, single leaf child, LR direction.
runCase("compound_single_leaf_lr", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("g", {});
    g.setParent("a", "g");
}, { rankdir: "LR" });

// 23. Compound, three-leaf chain inside a cluster, TB.
runCase("compound_chain_tb", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setNode("g", {});
    g.setParent("a", "g");
    g.setParent("b", "g");
    g.setParent("c", "g");
    g.setEdge("a", "b");
    g.setEdge("b", "c");
});

// 24. Three-leaf chain, LR — mirrors mermaid state ④ "isolated cluster,
//     leaf-only, LR" — the case where the report saw the 5x5 swap.
runCase("compound_chain_lr", g => {
    g.setNode("a", { width: 50, height: 50 });
    g.setNode("b", { width: 50, height: 50 });
    g.setNode("c", { width: 50, height: 50 });
    g.setNode("g", {});
    g.setParent("a", "g");
    g.setParent("b", "g");
    g.setParent("c", "g");
    g.setEdge("a", "b");
    g.setEdge("b", "c");
}, { rankdir: "LR" });

// 25. Nested compound: outer { inner { a, b }, c }. TB.
runCase("compound_nested_tb", g => {
    g.setNode("a", { width: 40, height: 30 });
    g.setNode("b", { width: 40, height: 30 });
    g.setNode("c", { width: 40, height: 30 });
    g.setNode("inner", {});
    g.setNode("outer", {});
    g.setParent("a", "inner");
    g.setParent("b", "inner");
    g.setParent("inner", "outer");
    g.setParent("c", "outer");
    g.setEdge("a", "b");
    g.setEdge("b", "c");
});

// 26. Nested compound, LR rankdir.
runCase("compound_nested_lr", g => {
    g.setNode("a", { width: 40, height: 30 });
    g.setNode("b", { width: 40, height: 30 });
    g.setNode("c", { width: 40, height: 30 });
    g.setNode("inner", {});
    g.setNode("outer", {});
    g.setParent("a", "inner");
    g.setParent("b", "inner");
    g.setParent("inner", "outer");
    g.setParent("c", "outer");
    g.setEdge("a", "b");
    g.setEdge("b", "c");
}, { rankdir: "LR" });

// 27. Cross-cluster edge: a-child(g1) → b-child(g2). TB.
runCase("compound_cross_cluster_tb", g => {
    g.setNode("a", { width: 50, height: 30 });
    g.setNode("b", { width: 50, height: 30 });
    g.setNode("g1", {});
    g.setNode("g2", {});
    g.setParent("a", "g1");
    g.setParent("b", "g2");
    g.setEdge("a", "b");
});

// 28. Cross-cluster edge, LR.
runCase("compound_cross_cluster_lr", g => {
    g.setNode("a", { width: 50, height: 30 });
    g.setNode("b", { width: 50, height: 30 });
    g.setNode("g1", {});
    g.setNode("g2", {});
    g.setParent("a", "g1");
    g.setParent("b", "g2");
    g.setEdge("a", "b");
}, { rankdir: "LR" });

// 29. Empty inner cluster (no children of its own).
runCase("compound_empty_inner", g => {
    g.setNode("a", { width: 40, height: 30 });
    g.setNode("inner_empty", {});
    g.setNode("outer", {});
    g.setParent("a", "outer");
    g.setParent("inner_empty", "outer");
});

// 30. Fork/join inside a cluster — exercises ⑨ "+2 px" report case.
runCase("compound_fork_join", g => {
    g.setNode("a", { width: 40, height: 30 });
    g.setNode("l", { width: 40, height: 30 });
    g.setNode("r", { width: 40, height: 30 });
    g.setNode("z", { width: 40, height: 30 });
    g.setNode("g", {});
    g.setParent("l", "g");
    g.setParent("r", "g");
    g.setEdge("a", "l");
    g.setEdge("a", "r");
    g.setEdge("l", "z");
    g.setEdge("r", "z");
});

// Capture the upstream version + commit so reviewers can tell at a glance
// which dagre.js produced this baseline. `commit` falls back to "unknown" if
// ref/dagre-js is not a git checkout (e.g. unpacked tarball).
const refRoot = new URL('../ref/dagre-js/', import.meta.url);
const refPkg = JSON.parse(readFileSync(new URL('package.json', refRoot), 'utf8'));
let refCommit = 'unknown';
try {
    refCommit = execSync('git rev-parse HEAD', { cwd: refRoot, encoding: 'utf8' }).trim();
} catch {
    // ref/dagre-js is not a git checkout; leave commit as "unknown".
}

const output = {
    _meta: {
        upstream: refPkg.name ?? '@dagrejs/dagre',
        version: refPkg.version,
        commit: refCommit,
        generated_at: new Date().toISOString(),
        generator: 'cross-validate/generate_reference.mjs',
    },
    cases: testCases,
};

writeFileSync(
    new URL('./reference_data.json', import.meta.url),
    JSON.stringify(output, null, 2)
);

console.log(
    `Wrote ${testCases.length} cases generated from ${output._meta.upstream}@${output._meta.version} (${output._meta.commit.slice(0, 7)})`
);
