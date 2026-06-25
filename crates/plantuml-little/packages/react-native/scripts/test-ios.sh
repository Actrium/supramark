#!/bin/bash
#
# test-ios.sh — build and run the iOS native bridge XCTest on a Mac.
#
# The SupramarkPlantumlModule bridge marshals NSString <-> the C FFI;
# that layer only exists on the Apple platform and cannot be covered by
# the Rust or JS test suites. This script:
#
#   1. builds the supramark-plantuml-native static lib for the host arch,
#   2. compiles the bridge + XCTest into a macOS .xctest bundle against
#      the minimal RN header stubs in ios/__tests__/stubs,
#   3. runs it with `xcrun xctest`.
#
# Why `cargo rustc --crate-type staticlib` instead of `cargo build`:
# the crate also declares a `cdylib` artefact, whose link step pulls in
# the bundled Graphviz C/C++ (expat / zlib / libc++) and fails on a host
# without those wired into the rustc link line. The XCTest only needs the
# `.a` archive — building just the staticlib sidesteps the cdylib link
# while producing the exact artefact we link below.
#
# Requires: macOS, Xcode (xcrun/clang), a Rust toolchain (cargo), and the
# system `libexpat` + `libz` (both ship with the macOS SDK) — the Graphviz
# objects inside the static lib reference XML_* (expat) and crc32/deflate
# (zlib).
#
# Usage: crates/plantuml-little/packages/react-native/scripts/test-ios.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

NATIVE_MANIFEST="$REPO/crates/plantuml-little/packages/native/Cargo.toml"
NATIVE_INCLUDE="$REPO/crates/plantuml-little/packages/native/include"
IOS_DIR="$PKG/ios"
TEST_DIR="$IOS_DIR/__tests__"
STUBS="$TEST_DIR/stubs"

echo "==> [1/3] Building native static lib for host"
cargo rustc --manifest-path "$NATIVE_MANIFEST" --crate-type staticlib
LIB="$REPO/target/debug/libsupramark_plantuml_native.a"
[ -f "$LIB" ] || { echo "static lib not found at $LIB" >&2; exit 1; }

echo "==> [2/3] Compiling .xctest bundle"
SDK="$(xcrun --show-sdk-path)"
PLATFORM="$(xcrun --show-sdk-platform-path)"
FWPATH="$PLATFORM/Developer/Library/Frameworks"

OUT="$(mktemp -d)"
BUNDLE="$OUT/SupramarkPlantumlModuleTests.xctest"
mkdir -p "$BUNDLE/Contents/MacOS"
BIN="$BUNDLE/Contents/MacOS/SupramarkPlantumlModuleTests"

clang++ -g -O0 -fobjc-arc -std=c++17 \
  -isysroot "$SDK" \
  -I"$IOS_DIR" -I"$NATIVE_INCLUDE" -I"$STUBS" \
  -F"$FWPATH" -Wl,-rpath,"$FWPATH" \
  -bundle \
  "$IOS_DIR/SupramarkPlantumlModule.mm" \
  "$TEST_DIR/SupramarkPlantumlModuleTests.mm" \
  "$LIB" \
  -lexpat -lz \
  -framework Foundation -framework XCTest \
  -framework CoreFoundation -framework Security \
  -o "$BIN"

cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>SupramarkPlantumlModuleTests</string>
  <key>CFBundleIdentifier</key><string>com.supramark.plantumlnative.tests</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>SupramarkPlantumlModuleTests</string>
  <key>CFBundlePackageType</key><string>BNDL</string>
  <key>CFBundleVersion</key><string>1</string>
</dict>
</plist>
PLIST

echo "==> [3/3] Running tests"
exec xcrun xctest "$BUNDLE"
