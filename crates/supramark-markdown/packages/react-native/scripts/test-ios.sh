#!/bin/bash
#
# test-ios.sh — build and run the iOS native bridge XCTest on a Mac.
#
# The SupramarkMarkdownModule bridge marshals NSString <-> the C FFI; that
# layer only exists on the Apple platform and cannot be covered by the
# Rust or JS test suites. This script:
#
#   1. builds the supramark-markdown-native static lib for the host arch,
#   2. compiles the bridge + XCTest into a macOS .xctest bundle against
#      the minimal RN header stubs in ios/__tests__/stubs,
#   3. runs it with `xcrun xctest`.
#
# Requires: macOS, Xcode (xcrun/clang), a Rust toolchain (cargo).
# Usage: crates/supramark-markdown/packages/react-native/scripts/test-ios.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

NATIVE_MANIFEST="$REPO/crates/supramark-markdown/packages/native/Cargo.toml"
NATIVE_INCLUDE="$REPO/crates/supramark-markdown/packages/native/include"
IOS_DIR="$PKG/ios"
TEST_DIR="$IOS_DIR/__tests__"
STUBS="$TEST_DIR/stubs"

echo "==> [1/3] Building native static lib for host"
cargo build --manifest-path "$NATIVE_MANIFEST"
LIB="$REPO/target/debug/libsupramark_markdown_native.a"
[ -f "$LIB" ] || { echo "static lib not found at $LIB" >&2; exit 1; }

echo "==> [2/3] Compiling .xctest bundle"
SDK="$(xcrun --show-sdk-path)"
PLATFORM="$(xcrun --show-sdk-platform-path)"
FWPATH="$PLATFORM/Developer/Library/Frameworks"

OUT="$(mktemp -d)"
BUNDLE="$OUT/SupramarkMarkdownModuleTests.xctest"
mkdir -p "$BUNDLE/Contents/MacOS"
BIN="$BUNDLE/Contents/MacOS/SupramarkMarkdownModuleTests"

clang++ -g -O0 -fobjc-arc -std=c++17 \
  -isysroot "$SDK" \
  -I"$IOS_DIR" -I"$NATIVE_INCLUDE" -I"$STUBS" \
  -F"$FWPATH" -Wl,-rpath,"$FWPATH" \
  -bundle \
  "$IOS_DIR/SupramarkMarkdownModule.mm" \
  "$TEST_DIR/SupramarkMarkdownModuleTests.mm" \
  "$LIB" \
  -framework Foundation -framework XCTest \
  -framework CoreFoundation -framework Security \
  -o "$BIN"

cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>SupramarkMarkdownModuleTests</string>
  <key>CFBundleIdentifier</key><string>com.supramark.markdownnative.tests</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>SupramarkMarkdownModuleTests</string>
  <key>CFBundlePackageType</key><string>BNDL</string>
  <key>CFBundleVersion</key><string>1</string>
</dict>
</plist>
PLIST

echo "==> [3/3] Running tests"
exec xcrun xctest "$BUNDLE"
