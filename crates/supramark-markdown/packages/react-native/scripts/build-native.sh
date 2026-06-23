#!/usr/bin/env bash
# 编译 supramark-markdown-native 的 iOS / Android 产物，并打包成
# RN wrapper 包可消费的 xcframework + jniLibs 布局。
#
# 用法：
#   scripts/build-native.sh           # 默认构建 iOS + Android（如果环境齐全）
#   scripts/build-native.sh --ios     # 只构建 iOS
#   scripts/build-native.sh --android # 只构建 Android
#
# 前置条件：
#   - Rust 工具链 + 已安装 target：
#       rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#       rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
#   - Android 还需要 cargo-ndk（cargo install cargo-ndk）和 $ANDROID_NDK_HOME
#   - Xcode（用于 xcodebuild / lipo）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
# PKG_DIR = crates/supramark-markdown/packages/react-native
# 往上 4 级到 repo root: packages → supramark-markdown → crates → root
REPO_ROOT="$(cd "${PKG_DIR}/../../../.." && pwd)"
CRATE_NAME="supramark-markdown-native"
LIB_NAME="libsupramark_markdown_native.a"
SO_NAME="libsupramark_markdown_native.so"
HEADER_DIR="${PKG_DIR}/../native/include"
XCFRAMEWORK_OUT="${REPO_ROOT}/target/ios-xcframeworks/SupramarkMarkdown.xcframework"

BUILD_IOS=0
BUILD_ANDROID=0

if [[ $# -eq 0 ]]; then
  BUILD_IOS=1
  BUILD_ANDROID=1
fi
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ios)     BUILD_IOS=1; shift ;;
    --android) BUILD_ANDROID=1; shift ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

echo "==> Repo root: ${REPO_ROOT}"
echo "==> Package:   ${PKG_DIR}"

# 所有 cargo 命令在 repo root 跑，确保找到 workspace Cargo.toml
cd "${REPO_ROOT}"

# ----------------------------------------------------------------------------
# iOS
# ----------------------------------------------------------------------------
if [[ "${BUILD_IOS}" -eq 1 ]]; then
  echo ""
  echo "==> [iOS] Building device (aarch64-apple-ios)"
  cargo build --release --target aarch64-apple-ios -p "${CRATE_NAME}"

  echo ""
  echo "==> [iOS] Building simulator (aarch64-apple-ios-sim)"
  cargo build --release --target aarch64-apple-ios-sim -p "${CRATE_NAME}"

  echo ""
  echo "==> [iOS] Building simulator (x86_64-apple-ios)"
  cargo build --release --target x86_64-apple-ios -p "${CRATE_NAME}"

  echo ""
  echo "==> [iOS] Lipo sim slices → target/ios-sim-universal"
  mkdir -p "${REPO_ROOT}/target/ios-sim-universal/release"
  lipo -create \
    "${REPO_ROOT}/target/aarch64-apple-ios-sim/release/${LIB_NAME}" \
    "${REPO_ROOT}/target/x86_64-apple-ios/release/${LIB_NAME}" \
    -output "${REPO_ROOT}/target/ios-sim-universal/release/${LIB_NAME}"

  echo ""
  echo "==> [iOS] Assembling xcframework"
  "${REPO_ROOT}/scripts/build-ios-xcframework.sh" \
    "${CRATE_NAME}" \
    "${HEADER_DIR}" \
    "${LIB_NAME}" \
    "${XCFRAMEWORK_OUT}"
fi

# ----------------------------------------------------------------------------
# Android
# ----------------------------------------------------------------------------
if [[ "${BUILD_ANDROID}" -eq 1 ]]; then
  if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo ""
    echo "==> [Android] SKIPPED: cargo-ndk not found."
    echo "    Install with: cargo install cargo-ndk"
    echo "    Then re-run: scripts/build-native.sh --android"
  else
    if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
      echo "==> [Android] SKIPPED: ANDROID_NDK_HOME not set."
      exit 1
    fi
    echo ""
    echo "==> [Android] Building 4 ABIs via cargo-ndk"
    cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
      build --release -p "${CRATE_NAME}"
  fi
fi

# ----------------------------------------------------------------------------
# Stage into RN package layout
# ----------------------------------------------------------------------------
echo ""
echo "==> Staging artefacts into RN package (prepare-native.js)"
cd "${PKG_DIR}"
node scripts/prepare-native.js

echo ""
echo "==> Done."
echo "    iOS xcframework: ${XCFRAMEWORK_OUT}"
echo "    Android jniLibs: ${PKG_DIR}/android/src/main/jniLibs/"
