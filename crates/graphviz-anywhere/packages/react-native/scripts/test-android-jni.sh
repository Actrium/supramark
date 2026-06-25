#!/bin/bash
#
# test-android-jni.sh — build and run the JNI bridge test on a host JVM.
#
# graphviz_jni.c marshals Java Strings / a long context handle <-> the
# graphviz_api C ABI. That C layer is plain JNI (only <android/log.h> is
# Android-specific, stubbed here), so it can be exercised on a host JVM
# without an Android NDK, emulator, gradle or React Native.
#
# Unlike the other engines, graphviz-anywhere has NO cargo native crate:
# its host static lib `libgraphviz_api.a` is produced by
# crates/graphviz-anywhere/scripts/build-linux.sh (which builds Graphviz
# from source — slow). This script:
#
#   1. locates a prebuilt host libgraphviz_api.a (under
#      crates/graphviz-anywhere/output/linux-<arch>/lib), or builds one
#      via build-linux.sh if absent,
#   2. compiles graphviz_jni.c into a host JNI shared library (against the
#      host JDK headers + the minimal <android/log.h> stub),
#   3. compiles the host test harness and runs it under `java`.
#
# It covers the C JNI marshalling, NOT the production Java Promise/Executor
# wrapper and NOT the Android ART runtime — an on-device/emulator
# instrumented test would be a separate, heavier effort.
#
# Requires: a JDK (JAVA_HOME or discoverable) and a C/C++ toolchain. Usage:
#   crates/graphviz-anywhere/packages/react-native/scripts/test-android-jni.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

GV_ROOT="$REPO/crates/graphviz-anywhere"
NATIVE_INCLUDE="$GV_ROOT/capi"
JNI_DIR="$PKG/android/src/main/jni"
TEST_DIR="$JNI_DIR/__tests__"
STUBS="$TEST_DIR/stubs"

case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) ARCH="$(uname -m)" ;;
esac

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

echo "==> [1/3] Locating host graphviz static lib"
LIB="$GV_ROOT/output/linux-$ARCH/lib/libgraphviz_api.a"
if [ ! -f "$LIB" ]; then
  echo "    no prebuilt $LIB; building Graphviz from source (slow)…"
  bash "$GV_ROOT/scripts/build-linux.sh" --arch "$ARCH"
fi
[ -f "$LIB" ] || { echo "graphviz static lib not found at $LIB; run scripts/build-linux.sh" >&2; exit 1; }
echo "    using $LIB"

echo "==> [2/3] Compiling JNI shared library"
OUT="$(mktemp -d)"
# The unified libgraphviz_api.a bundles Graphviz's C++ libraries, so the JNI
# .so must be linked with a C++ driver (g++) to resolve libstdc++ symbols,
# plus -lm -lz -lexpat (HTML labels + DEFLATE) — matching build-linux.sh.
case "$(uname)" in
  Darwin) SO="$OUT/libgraphviz_jni.dylib"; SHARED=(-dynamiclib); OSLIBS=(-lz -lexpat -framework CoreFoundation);;
  *)      SO="$OUT/libgraphviz_jni.so";    SHARED=(-shared);      OSLIBS=(-lm -lz -lexpat -lpthread -ldl);;
esac
# Compile the JNI C source to an object first (as C), then link the unified
# archive with a C++ driver. Doing both in one g++ invocation would force the
# .a onto the C++ compiler's input as source; splitting the steps avoids that.
CC_FOR_JNI="${CC:-cc}"
"$CC_FOR_JNI" -c -fPIC -O0 -g \
  -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/darwin" -I"$JAVA_HOME/include/linux" \
  -I"$NATIVE_INCLUDE" -I"$STUBS" \
  "$JNI_DIR/graphviz_jni.c" -o "$OUT/graphviz_jni.o"
${CXX:-g++} "${SHARED[@]}" -fPIC \
  "$OUT/graphviz_jni.o" \
  -Wl,--whole-archive "$LIB" -Wl,--no-whole-archive \
  "${OSLIBS[@]}" \
  -o "$SO"

echo "==> [3/3] Compiling and running host JNI test"
CLASSES="$OUT/classes"
mkdir -p "$CLASSES"
"$JAVA_HOME/bin/javac" -d "$CLASSES" "$TEST_DIR/GraphvizModule.java"
exec "$JAVA_HOME/bin/java" --enable-native-access=ALL-UNNAMED \
  -cp "$CLASSES" com.graphviznative.GraphvizModule "$SO"
