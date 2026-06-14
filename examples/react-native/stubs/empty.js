// Stub for wasm web packages on the RN bundle. RN routes D2 / Mermaid /
// PlantUML through native FFI wrappers, while ECharts / Vega-Lite use pure JS
// SVG-string engines. The *-web wasm packages must never load on RN, but
// engines/src/* can still resolve those names statically. Re-exporting
// nothing leaves any accidental `await import(...)` resolvable to an empty
// object, and downstream code throws a clear error if the expected entry
// points are missing.
module.exports = {};
