#!/bin/bash
#
# test-android-jni.sh — build and run the JNI bridge test on a host JVM.
#
# supramark_d2_jni.c marshals a Java String <-> the C FFI. That C layer
# is plain JNI (only <android/log.h> is Android-specific, stubbed here), so
# it can be exercised on a host JVM without an Android NDK, emulator, gradle
# or React Native. This script:
#
#   1. builds the supramark-d2-native static lib for the host arch,
#   2. compiles supramark_d2_jni.c into a host JNI shared library
#      (against the host JDK headers + the minimal <android/log.h> stub),
#   3. compiles the host test harness and runs it under `java`.
#
# It covers the C JNI marshalling, NOT the production Java Promise/Executor
# wrapper and NOT the Android ART runtime — an on-device/emulator
# instrumented test would be a separate, heavier effort.
#
# Requires: a JDK (JAVA_HOME or discoverable), a C toolchain, a Rust
# toolchain (cargo). Usage:
#   crates/d2-little/packages/react-native/scripts/test-android-jni.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

NATIVE_MANIFEST="$REPO/crates/d2-little/packages/native/Cargo.toml"
NATIVE_INCLUDE="$REPO/crates/d2-little/packages/native/include"
JNI_DIR="$PKG/android/src/main/jni"
TEST_DIR="$JNI_DIR/__tests__"
STUBS="$TEST_DIR/stubs"

# Locate a JDK with headers (jni.h).
if [ -z "${JAVA_HOME:-}" ] || [ ! -f "${JAVA_HOME}/include/jni.h" ]; then
  JAVA_HOME="$(/usr/libexec/java_home 2>/dev/null || true)"
fi
if [ ! -f "${JAVA_HOME:-}/include/jni.h" ] && command -v brew >/dev/null 2>&1; then
  JAVA_HOME="$(brew --prefix openjdk 2>/dev/null)/libexec/openjdk.jdk/Contents/Home"
fi
# Linux fallback when /usr/libexec/java_home is absent.
if [ ! -f "${JAVA_HOME:-}/include/jni.h" ] && [ -f /usr/lib/jvm/java-1.17.0-openjdk-amd64/include/jni.h ]; then
  JAVA_HOME=/usr/lib/jvm/java-1.17.0-openjdk-amd64
fi
[ -f "${JAVA_HOME:-}/include/jni.h" ] || { echo "no JDK headers; set JAVA_HOME to a JDK with include/jni.h" >&2; exit 1; }
echo "JAVA_HOME=$JAVA_HOME"

echo "==> [1/3] Building native static lib for host"
cargo build --manifest-path "$NATIVE_MANIFEST"
LIB="$REPO/target/debug/libsupramark_d2_native.a"
[ -f "$LIB" ] || { echo "static lib not found at $LIB" >&2; exit 1; }

echo "==> [2/3] Compiling JNI shared library"
OUT="$(mktemp -d)"
case "$(uname)" in
  Darwin) SO="$OUT/libsupramark_d2_jni.dylib"; SHARED=(-dynamiclib); OSLIBS=(-framework CoreFoundation -framework Security);;
  *)      SO="$OUT/libsupramark_d2_jni.so";    SHARED=(-shared);      OSLIBS=(-lm -lpthread -ldl);;
esac
clang "${SHARED[@]}" -fPIC -O0 -g \
  -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/darwin" -I"$JAVA_HOME/include/linux" \
  -I"$NATIVE_INCLUDE" -I"$STUBS" \
  "$JNI_DIR/supramark_d2_jni.c" \
  "$LIB" "${OSLIBS[@]}" \
  -o "$SO"

echo "==> [3/3] Compiling and running host JNI test"
CLASSES="$OUT/classes"
mkdir -p "$CLASSES"
"$JAVA_HOME/bin/javac" -d "$CLASSES" "$TEST_DIR/SupramarkD2Module.java"
exec "$JAVA_HOME/bin/java" --enable-native-access=ALL-UNNAMED \
  -cp "$CLASSES" com.supramark.d2native.SupramarkD2Module "$SO"
