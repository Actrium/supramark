//! cose-bilkent layout integration — byte-exact mindmap layout via embedded
//! cytoscape.js + cose-bilkent inside rquickjs.
//!
//! Mermaid drives the JavaScript `cose-bilkent` extension to lay out mindmap
//! diagrams. A pure Rust port of the multi-level coarsening / spring embedder
//! is unlikely to be byte-exact (FP accumulation order, multi-stage tiling,
//! randomisation), so we embed the upstream JS bundle and run the layout
//! inside an embedded QuickJS runtime — same approach as the `katex` feature.
//!
//! Spike at `/tmp/spike-cytoscape` confirmed cytoscape headless +
//! cose-bilkent runs in rquickjs with only a 15-LOC stub (no-op `console`,
//! `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`). No DOM
//! shim is required when we set `headless: true` directly.
//!
//! Vendored sources (all MIT):
//!   * `vendor/cytoscape.umd.js`        — 1.15 MB
//!   * `vendor/layout-base.js`          — 115 KB
//!   * `vendor/cose-base.js`            —  45 KB
//!   * `vendor/cytoscape-cose-bilkent.js` — 16 KB

pub mod render;

pub use render::{layout, Edge, Graph, LayoutError, Node, PositionedEdge, PositionedNode};

#[cfg(test)]
mod tests {
    use super::*;

    /// Math.pow byte-exactness vs V8 oracle. The fdlibm-based shim installed
    /// by render.rs must match Node/V8 bit-for-bit on the inputs cose-bilkent
    /// actually feeds to it (`pow(coolingCycle, log(96)/log(25))` for
    /// coolingCycle = 1..25), plus a sanity sweep of generic inputs.
    ///
    /// Oracle bits captured from `node -e` on a Linux x86-64 host. The
    /// hex strings encode the IEEE-754 high-then-low word order — i.e. what
    /// you'd see by doing `(BigInt(u32[1]) << 32n) | BigInt(u32[0])` in JS.
    #[test]
    fn math_pow_matches_v8_oracle() {
        use rquickjs::{CatchResultExt, Context, Runtime};

        // (base, exp_bits, expected_result_bits).
        // exp_bits = `Math.log(96)/Math.log(25)` for the cose-bilkent set.
        let exp_bits: u64 = 0x3FF6B01AFE2E13CA; // 1.4179944924264754
        let cose_cases: &[(f64, u64, u64)] = &[
            (1.0, exp_bits, 0x3FF0000000000000),
            (2.0, exp_bits, 0x40056089DD519CFA),
            (3.0, exp_bits, 0x4012FE738AE11DED),
            (4.0, exp_bits, 0x401C8FB05FF9F978),
            (5.0, exp_bits, 0x4023988E1409212F),
            (6.0, exp_bits, 0x4029609205019194),
            (7.0, exp_bits, 0x402F93C6ECD86918),
            (8.0, exp_bits, 0x40331475DCD04673),
            (9.0, exp_bits, 0x40368C5290364FE6),
            (10.0, exp_bits, 0x403A2E76A813DCE9),
            (11.0, exp_bits, 0x403DF85C8B3375D5),
            (12.0, exp_bits, 0x4040F3EEDE4CDB0E),
            (13.0, exp_bits, 0x4042FD8EF402A79A),
            (14.0, exp_bits, 0x4045183DEB4552FC),
            (15.0, exp_bits, 0x40474343289D05A8),
            (16.0, exp_bits, 0x40497DF9DC1B5F95),
            (17.0, exp_bits, 0x404BC7CDC21E8B8E),
            (18.0, exp_bits, 0x404E20389610B991),
            (19.0, exp_bits, 0x40504360048E30C7),
            (20.0, exp_bits, 0x40517D7A0E44EDA9),
            (21.0, exp_bits, 0x4052BE36E450027D),
            (22.0, exp_bits, 0x40540566EF8F427A),
            (23.0, exp_bits, 0x405552DE0E606C60),
            (24.0, exp_bits, 0x4056A6733027C8B2),
            (25.0, exp_bits, 0x4058000000000002),
        ];
        // Generic sanity: special / small-integer / negative-exponent.
        let sanity_cases: &[(u64, u64, u64)] = &[
            (2.0_f64.to_bits(), 0.5_f64.to_bits(), 0x3FF6A09E667F3BCD), // sqrt(2)
            (
                std::f64::consts::E.to_bits(),
                std::f64::consts::PI.to_bits(),
                0x403724046EB09338,
            ),
            (10.0_f64.to_bits(), (-3.0_f64).to_bits(), 0x3F50624DD2F1A9FC),
            (2.0_f64.to_bits(), 10.0_f64.to_bits(), 0x4090000000000000),
            (3.0_f64.to_bits(), 3.0_f64.to_bits(), 0x403B000000000000),
            (0.5_f64.to_bits(), 2.0_f64.to_bits(), 0x3FD0000000000000),
        ];

        let rt = Runtime::new().expect("rt");
        let ctx = Context::full(&rt).expect("ctx");
        ctx.with(|ctx| {
            ctx.eval::<(), _>(super::render::POW_SHIM_FOR_TEST)
                .catch(&ctx)
                .expect("pow shim install");
            // Helper: take base bits + exp bits, return result bits as
            // hex string (high-then-low). We round-trip via bit_cast to
            // avoid any literal-parse drift.
            ctx.eval::<(), _>(
                r#"
                globalThis.__buf = new ArrayBuffer(8);
                globalThis.__f64 = new Float64Array(globalThis.__buf);
                globalThis.__u32 = new Uint32Array(globalThis.__buf);
                globalThis.__bitsToFloat = function (hi, lo) {
                    globalThis.__u32[1] = hi >>> 0;
                    globalThis.__u32[0] = lo >>> 0;
                    return globalThis.__f64[0];
                };
                globalThis.__floatToBits = function (d) {
                    globalThis.__f64[0] = d;
                    return [globalThis.__u32[1] >>> 0, globalThis.__u32[0] >>> 0];
                };
                globalThis.__powBits = function (bhi, blo, ehi, elo) {
                    var b = globalThis.__bitsToFloat(bhi, blo);
                    var e = globalThis.__bitsToFloat(ehi, elo);
                    var r = Math.pow(b, e);
                    return globalThis.__floatToBits(r);
                };
                "#,
            )
            .catch(&ctx)
            .expect("install helpers");

            let pow_bits = |a: u64, b: u64| -> u64 {
                let (ahi, alo) = ((a >> 32) as u32, a as u32);
                let (bhi, blo) = ((b >> 32) as u32, b as u32);
                ctx.globals().set("__bhi", ahi).unwrap();
                ctx.globals().set("__blo", alo).unwrap();
                ctx.globals().set("__ehi", bhi).unwrap();
                ctx.globals().set("__elo", blo).unwrap();
                let v: rquickjs::Value = ctx
                    .eval(r#"globalThis.__powBits(__bhi, __blo, __ehi, __elo)"#)
                    .catch(&ctx)
                    .expect("eval pow");
                let arr = v.into_array().expect("array");
                let hi: u32 = arr.get(0).expect("hi");
                let lo: u32 = arr.get(1).expect("lo");
                ((hi as u64) << 32) | (lo as u64)
            };

            let mut failures: Vec<String> = Vec::new();
            for &(base, exp_b, want) in cose_cases {
                let got = pow_bits(base.to_bits(), exp_b);
                if got != want {
                    failures.push(format!(
                        "pow({}, {:#018x}): got {:#018x}, want {:#018x}",
                        base, exp_b, got, want
                    ));
                }
            }
            for &(b_bits, e_bits, want) in sanity_cases {
                let got = pow_bits(b_bits, e_bits);
                if got != want {
                    failures.push(format!(
                        "pow({:#018x}, {:#018x}): got {:#018x}, want {:#018x}",
                        b_bits, e_bits, got, want
                    ));
                }
            }
            if !failures.is_empty() {
                panic!(
                    "Math.pow shim mismatched V8 oracle on {} input(s):\n{}",
                    failures.len(),
                    failures.join("\n")
                );
            }
        });
    }

    /// Math.sin / Math.cos byte-exactness vs V8 oracle. Sweeps 200 inputs
    /// (0.1 .. 20.0 in 0.1 steps) with the fdlibm-based shims installed and
    /// requires every bit pattern to match Node/V8.
    #[test]
    fn math_sin_cos_match_v8_oracle() {
        use rquickjs::{CatchResultExt, Context, Runtime};
        let rt = Runtime::new().expect("rt");
        let ctx = Context::full(&rt).expect("ctx");
        ctx.with(|ctx| {
            ctx.eval::<(), _>(super::render::POW_SHIM_FOR_TEST)
                .catch(&ctx)
                .expect("pow shim");
            ctx.eval::<(), _>(super::render::SINCOS_SHIM_FOR_TEST)
                .catch(&ctx)
                .expect("sincos shim");
            let qjs_dump: String = ctx
                .eval(
                    r#"
                    (function(){
                        var b = new ArrayBuffer(8);
                        var f = new Float64Array(b);
                        var u = new Uint32Array(b);
                        function bits(d) { f[0]=d; return ((BigInt(u[1])<<32n)|BigInt(u[0])).toString(16).padStart(16,"0"); }
                        var lines = [];
                        for (var i=1;i<=200;i++) {
                            var v = i*0.1;
                            lines.push("sin("+v.toFixed(1)+")="+bits(Math.sin(v)));
                            lines.push("cos("+v.toFixed(1)+")="+bits(Math.cos(v)));
                        }
                        return lines.join("\n");
                    })()
                    "#,
                )
                .catch(&ctx)
                .expect("eval sweep");
            // Embedded V8 oracle, captured offline via Node:
            //   `node -e "for (let i=1;i<=200;i++){const v=i*0.1; ...}"`
            let v8_oracle = include_str!("v8_sincos_oracle.txt");
            if qjs_dump.trim() != v8_oracle.trim() {
                let mut diffs: Vec<String> = Vec::new();
                for (i, (a, b)) in qjs_dump
                    .lines()
                    .zip(v8_oracle.lines())
                    .enumerate()
                {
                    if a != b {
                        diffs.push(format!("line {}: shim={} v8={}", i, a, b));
                    }
                }
                panic!(
                    "sin/cos shim diverges from V8 on {} input(s):\n{}",
                    diffs.len(),
                    diffs.join("\n")
                );
            }
        });
    }

    /// Smoke test — runs the spike's 3-node graph through the harness twice
    /// and checks that the output is deterministic across runs.
    #[test]
    fn smoke_3_nodes_deterministic() {
        let g = Graph {
            nodes: vec![
                Node {
                    id: "a".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
                Node {
                    id: "b".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
                Node {
                    id: "c".into(),
                    label: String::new(),
                    width: 50.0,
                    height: 30.0,
                    padding: 0.0,
                },
            ],
            edges: vec![
                Edge {
                    id: "ab".into(),
                    source: "a".into(),
                    target: "b".into(),
                },
                Edge {
                    id: "ac".into(),
                    source: "a".into(),
                    target: "c".into(),
                },
            ],
        };
        let out1 = layout(&g).expect("first layout run");
        let out2 = layout(&g).expect("second layout run");
        assert_eq!(out1.nodes.len(), 3);
        assert_eq!(out1.edges.len(), 2);
        for (a, b) in out1.nodes.iter().zip(out2.nodes.iter()) {
            assert_eq!(a.id, b.id);
            assert!(
                (a.x - b.x).abs() < 1e-9,
                "node {} x drift: {} vs {}",
                a.id,
                a.x,
                b.x
            );
            assert!(
                (a.y - b.y).abs() < 1e-9,
                "node {} y drift: {} vs {}",
                a.id,
                a.y,
                b.y
            );
        }
    }
}
