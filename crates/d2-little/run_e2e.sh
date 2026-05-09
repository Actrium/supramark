#!/bin/bash
# Run e2e tests with increased stack size
# Usage: ./run_e2e.sh
export RUST_MIN_STACK=67108864  # 64MB stack
cargo test -p d2-little --test e2e_runner -- --nocapture "$@"
