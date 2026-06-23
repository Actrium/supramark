#!/bin/bash
#
# test-android-device.sh — run the JNI bridge test on a connected Android
# device or emulator (real ART), no gradle / React Native app needed.
#
# Complements scripts/test-android-jni.sh (which runs the same harness on a
# host JVM). This one cross-compiles the native crate for the device ABI with
# cargo-ndk, links supramark_markdown_jni.c into a self-contained .so with the
# NDK toolchain, dexes the host harness with d8, pushes both to the device and
# runs them on ART via app_process. It exercises the exact JNI binding on the
# real Android runtime — GetByteArrayElements, NewByteArray, etc.
#
# Requires: a JDK, Android SDK (ANDROID_HOME) with platform-tools + NDK +
# build-tools, cargo + cargo-ndk, the matching `rustup target`, and a single
# device/emulator visible to `adb`.
#
# Usage:
#   crates/supramark-markdown/packages/react-native/scripts/test-android-device.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
PKG="$(cd "$HERE/.." && pwd)"
REPO="$(cd "$PKG" && git rev-parse --show-toplevel)"

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
: "${ANDROID_NDK_HOME:=$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)}"
ADB="$ANDROID_HOME/platform-tools/adb"
NATIVE_MANIFEST="$REPO/crates/supramark-markdown/packages/native/Cargo.toml"
NATIVE_INCLUDE="$REPO/crates/supramark-markdown/packages/native/include"
JNI_DIR="$PKG/android/src/main/jni"
HARNESS="$JNI_DIR/__tests__/SupramarkMarkdownModule.java"

# Locate a JDK with javac.
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "${JAVA_HOME}/bin/javac" ]; then
  JAVA_HOME="$(/usr/libexec/java_home 2>/dev/null || true)"
fi
if [ ! -x "${JAVA_HOME:-}/bin/javac" ] && command -v brew >/dev/null 2>&1; then
  JAVA_HOME="$(brew --prefix openjdk 2>/dev/null)/libexec/openjdk.jdk/Contents/Home"
fi
[ -x "${JAVA_HOME:-}/bin/javac" ] || { echo "no JDK (javac) found; set JAVA_HOME" >&2; exit 1; }

[ -n "$("$ADB" devices | sed '1d' | grep -w device || true)" ] || { echo "no adb device/emulator connected" >&2; exit 1; }

# Map the device ABI to its rustup target + NDK clang prefix.
ABI="$("$ADB" shell getprop ro.product.cpu.abi | tr -d '\r')"
case "$ABI" in
  arm64-v8a)   RUST_TRIPLE=aarch64-linux-android;     NDK_CLANG=aarch64-linux-android24-clang ;;
  armeabi-v7a) RUST_TRIPLE=armv7-linux-androideabi;   NDK_CLANG=armv7a-linux-androideabi24-clang ;;
  x86_64)      RUST_TRIPLE=x86_64-linux-android;      NDK_CLANG=x86_64-linux-android24-clang ;;
  x86)         RUST_TRIPLE=i686-linux-android;        NDK_CLANG=i686-linux-android24-clang ;;
  *) echo "unsupported device ABI: $ABI" >&2; exit 1 ;;
esac
case "$(uname)" in Darwin) NDK_HOST=darwin-x86_64 ;; *) NDK_HOST=linux-x86_64 ;; esac
CLANG="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_HOST/bin/$NDK_CLANG"
echo "device ABI=$ABI  rust=$RUST_TRIPLE"

OUT="$(mktemp -d)"

echo "==> [1/4] Cross-compiling native static lib ($ABI) with cargo-ndk"
cargo ndk -t "$ABI" build --manifest-path "$NATIVE_MANIFEST"
LIBA="$REPO/target/$RUST_TRIPLE/debug/libsupramark_markdown_native.a"
[ -f "$LIBA" ] || { echo "static lib not found at $LIBA" >&2; exit 1; }

echo "==> [2/4] Linking self-contained libsupramark_markdown_jni.so (NDK)"
SO="$OUT/libsupramark_markdown_jni.so"
"$CLANG" -shared -fPIC -O0 -g -I"$NATIVE_INCLUDE" \
  "$JNI_DIR/supramark_markdown_jni.c" "$LIBA" -llog -o "$SO"

echo "==> [3/4] Building dex harness with d8"
CLASSES="$OUT/classes"; mkdir -p "$CLASSES"
"$JAVA_HOME/bin/javac" --release 17 -d "$CLASSES" "$HARNESS"
D8="$(ls -d "$ANDROID_HOME"/build-tools/* | sort -V | tail -1)/d8"
"$D8" --output "$OUT/harness.jar" "$CLASSES/com/supramark/markdownnative/SupramarkMarkdownModule.class"

echo "==> [4/4] Pushing to device and running on ART"
"$ADB" push "$SO" /data/local/tmp/ >/dev/null
"$ADB" push "$OUT/harness.jar" /data/local/tmp/ >/dev/null
exec "$ADB" shell "cd /data/local/tmp && CLASSPATH=harness.jar app_process /system/bin com.supramark.markdownnative.SupramarkMarkdownModule /data/local/tmp/libsupramark_markdown_jni.so"
