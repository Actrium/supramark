#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export CARGO_TERM_COLOR=never

run_test() {
  local filter="$1"
  local tmp
  tmp="$(mktemp)"
  if ! cargo test "$filter" -- --exact --nocapture >"$tmp" 2>&1; then
    cat "$tmp"
    rm -f "$tmp"
    exit 1
  fi
  rg '^\[(flowchart|er|requirement|state|block|class)\] (scan|byte-exact=|fixtures=|render-failures)' "$tmp" || true
  rm -f "$tmp"
}

if ! cargo test --quiet >/dev/null 2>&1; then
  cargo test --quiet
  exit 1
fi

echo "[status] baseline=cargo test --quiet ok"
run_test "flowchart_parser_roundtrips_all_fixtures"
run_test "flowchart_byte_exact_sweep"
run_test "render::svg_er::tests::byte_exact_sweep"
run_test "render::svg_requirement::tests::byte_exact_sweep_reports_progress"
run_test "render::svg_state::tests::reports_byte_exact_pass_count"
run_test "render::svg_block::tests::byte_exact_sweep"
run_test "render::svg_class::tests::byte_exact_sweep"
