#!/bin/bash
#
# test-ios.sh — build and run the iOS native bridge XCTest on a Mac.
#
# The GraphvizModule bridge marshals NSString DOT/engine/format <-> the C
# FFI (gv_render over a gv_context_t); that layer only exists on the Apple
# platform and cannot be covered by the Rust or JS test suites. This
# script:
#
#   1. uses the prebuilt Graphviz static lib produced by
#      scripts/build-macos.sh (output/macos-universal/lib/libgraphviz_api.a),
#   2. compiles the ObjC bridge (.m) + the ObjC++ XCTest (.mm) into a
#      macOS .xctest bundle against the minimal RN header stubs in
#      ios/__tests__/stubs,
#   3. runs it with `xcrun xctest`.
#
# Unlike the other engines there is no cargo crate here: Graphviz is built
# from C/C++ source by scripts/build-macos.sh. If the prebuilt archive is
# missing, run that script first (it is slow — Graphviz from source).
#
# The Graphviz objects reference expat (XML_*) and zlib (crc32/deflate),
# and the C++ TU pulls in libc++, so we link -lexpat -lz -lc++ (all ship
# with the macOS SDK).
#
# Requires: macOS, Xcode (xcrun/clang). No Rust toolchain needed.
# Usage: crates/graphviz-anywhere/packages/react-native/scripts/test-ios.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

GV_OUT="$REPO/crates/graphviz-anywhere/output/macos-universal"
NATIVE_INCLUDE="$GV_OUT/include"
LIB="$GV_OUT/lib/libgraphviz_api.a"
IOS_DIR="$PKG/ios"
TEST_DIR="$IOS_DIR/__tests__"
STUBS="$TEST_DIR/stubs"

echo "==> [1/3] Locating prebuilt Graphviz static lib"
if [ ! -f "$LIB" ]; then
  echo "static lib not found at $LIB" >&2
  echo "Build it first: crates/graphviz-anywhere/scripts/build-macos.sh" >&2
  exit 1
fi

echo "==> [2/3] Compiling .xctest bundle"
SDK="$(xcrun --show-sdk-path)"
PLATFORM="$(xcrun --show-sdk-platform-path)"
FWPATH="$PLATFORM/Developer/Library/Frameworks"

OUT="$(mktemp -d)"
BUNDLE="$OUT/GraphvizModuleTests.xctest"
mkdir -p "$BUNDLE/Contents/MacOS"
BIN="$BUNDLE/Contents/MacOS/GraphvizModuleTests"

# The bridge is Objective-C (.m); the test is Objective-C++ (.mm). Compile
# each TU to an object file with the right language, then link them.
clang -g -O0 -fobjc-arc -x objective-c \
  -isysroot "$SDK" \
  -I"$IOS_DIR" -I"$NATIVE_INCLUDE" -I"$STUBS" \
  -c "$IOS_DIR/GraphvizModule.m" \
  -o "$OUT/GraphvizModule.o"

clang++ -g -O0 -fobjc-arc -std=c++17 -x objective-c++ \
  -isysroot "$SDK" \
  -I"$IOS_DIR" -I"$NATIVE_INCLUDE" -I"$STUBS" \
  -F"$FWPATH" \
  -c "$TEST_DIR/GraphvizModuleTests.mm" \
  -o "$OUT/GraphvizModuleTests.o"

clang++ -g -O0 -fobjc-arc \
  -isysroot "$SDK" \
  -F"$FWPATH" -Wl,-rpath,"$FWPATH" \
  -bundle \
  "$OUT/GraphvizModule.o" \
  "$OUT/GraphvizModuleTests.o" \
  "$LIB" \
  -lexpat -lz -lc++ \
  -framework Foundation -framework XCTest \
  -framework CoreFoundation -framework Security \
  -o "$BIN"

cat > "$BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>GraphvizModuleTests</string>
  <key>CFBundleIdentifier</key><string>com.graphviznative.tests</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>GraphvizModuleTests</string>
  <key>CFBundlePackageType</key><string>BNDL</string>
  <key>CFBundleVersion</key><string>1</string>
</dict>
</plist>
PLIST

echo "==> [3/3] Running tests"
exec xcrun xctest "$BUNDLE"
