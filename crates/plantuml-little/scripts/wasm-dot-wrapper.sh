#!/usr/bin/env bash
# dot-compatible wrapper that funnels Java PlantUML's Graphviz calls
# through the same wasm Graphviz blob (@actrium/graphviz-anywhere-web,
# viz.wasm from Graphviz 14.1.5) that plantuml-little's Rust tests use.
#
# Shared-wasm pipeline rationale
# ------------------------------
# The reference-test suite compares SVGs produced by Java PlantUML
# v1.2026.2 against SVGs produced by this crate's Rust code. Both sides
# shell out to Graphviz. If the two sides talk to *different* Graphviz
# binaries (different versions, different builds), tiny sub-pixel
# geometry deltas appear in the SVGs and the tests flake. Forcing both
# sides through the same deterministic wasm blob makes Graphviz output
# byte-identical everywhere, so any remaining SVG divergence must be a
# real plantuml-little vs Java-PlantUML implementation difference — the
# kind of bug this suite is meant to catch.
#
# Usage
# -----
# Point Java PlantUML at this script via the well-known environment
# variable:
#
#     export GRAPHVIZ_DOT=/abs/path/to/scripts/wasm-dot-wrapper.sh
#     java -jar plantuml.jar -tsvg -pipe < input.puml > out.svg
#
# Or via the `-graphvizdot` CLI flag. See tests/generate_reference.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
RUNNER="${REPO_ROOT}/tests/support/viz_wasm_dot_oneshot.mjs"
SUPPORT_DIR="${REPO_ROOT}/tests/support"

if [[ ! -f "${RUNNER}" ]]; then
    echo "wasm-dot-wrapper: runner not found at ${RUNNER}" >&2
    exit 127
fi

if [[ ! -d "${SUPPORT_DIR}/node_modules/@actrium/graphviz-anywhere-web" ]]; then
    echo "wasm-dot-wrapper: dependencies not installed. Run:" >&2
    echo "    cd ${SUPPORT_DIR} && npm install" >&2
    exit 127
fi

# Resolve `node`. Respect PLANTUML_LITTLE_NODE override so CI can pin a
# specific toolchain without touching PATH.
NODE_BIN="${PLANTUML_LITTLE_NODE:-node}"

# Silence Node's experimental-warning chatter; we leave stderr inheriting
# so real layout errors still surface.
export NODE_NO_WARNINGS=1

# Run from the support/ dir so the package resolution picks up the local
# tests/support/node_modules/ without needing a full resolve walk.
cd "${SUPPORT_DIR}"
exec "${NODE_BIN}" "${RUNNER}" "$@"
