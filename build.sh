#!/usr/bin/env bash
set -euo pipefail

profile="${1:-full}"
mode="${2:-build}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"

bun scripts/build-profile.ts "$profile"

# shellcheck disable=SC1091
source "$root/.supramark-build/profile.env"

cargo_args=(--no-default-features)
if [[ -n "${SUPRAMARK_CARGO_FEATURES:-}" ]]; then
  cargo_args+=(--features "$SUPRAMARK_CARGO_FEATURES")
fi

case "$mode" in
  build)
    cargo build -p supramark-markdown "${cargo_args[@]}"
    bun run build
    ;;
  test)
    cargo test -p supramark-markdown "${cargo_args[@]}"
    bun run test
    ;;
  rust)
    cargo build -p supramark-markdown "${cargo_args[@]}"
    ;;
  rust-test)
    cargo test -p supramark-markdown "${cargo_args[@]}"
    ;;
  generate)
    ;;
  *)
    echo "usage: ./build.sh [profile|profiles/name.json] [build|test|rust|rust-test|generate]" >&2
    exit 2
    ;;
esac
