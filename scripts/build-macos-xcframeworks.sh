#!/usr/bin/env bash
# Build the macOS arm64 + x86_64 static and dynamic libraries used by
# Supramark's React Native packages, merge them into universal binaries, and
# refresh the Apple XCFrameworks that already contain the iOS device and
# simulator slices.
#
# Existing iOS inputs under target/ are refreshed with a macOS slice when they
# are available. A clean macOS-only build does not require them because the RN
# macOS podspecs consume the staged dylibs directly.
#
# Usage:
#   scripts/build-macos-xcframeworks.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
MACOS_TARGET_DIR="${REPO_ROOT}/target/macos-universal/release"
APPLE_XCFRAMEWORK_DIR="${REPO_ROOT}/target/ios-xcframeworks"
GRAPHVIZ_ROOT="${REPO_ROOT}/crates/graphviz-anywhere"
GRAPHVIZ_MACOS_LIB_DIR="${GRAPHVIZ_ROOT}/output/macos-universal/lib"

# Each entry is crate|header directory|library stem|XCFramework name.
APPLE_NATIVE_MODULES=(
  "supramark-markdown-native|crates/supramark-markdown/packages/native/include|libsupramark_markdown_native|SupramarkMarkdown.xcframework"
  "supramark-d2-native|crates/d2-little/packages/native/include|libsupramark_d2_native|SupramarkD2.xcframework"
  "supramark-mermaid-native|crates/mermaid-little/packages/native/include|libsupramark_mermaid_native|SupramarkMermaid.xcframework"
  "supramark-plantuml-native|crates/plantuml-little/packages/native/include|libsupramark_plantuml_native|SupramarkPlantuml.xcframework"
)

cd "${REPO_ROOT}"

# Fail before the expensive Graphviz build when either Rust desktop standard
# library is unavailable; callers can install the exact missing target shown.
for required_target in aarch64-apple-darwin x86_64-apple-darwin; do
  if ! rustup target list --installed | grep -qx "${required_target}"; then
    echo "missing Rust target: ${required_target}" >&2
    echo "install it with: rustup target add ${required_target}" >&2
    exit 1
  fi
done

# D2, Mermaid and PlantUML link Graphviz statically. Build its universal macOS
# archive first so Cargo never falls back to an unavailable downloaded asset.
if [[ ! -f "${GRAPHVIZ_MACOS_LIB_DIR}/libgraphviz_api.a" || \
      ! -f "${GRAPHVIZ_MACOS_LIB_DIR}/libgraphviz_api.dylib" ]]; then
  echo "==> [macOS] Building Graphviz universal archive"
  "${GRAPHVIZ_ROOT}/scripts/build-macos.sh"
else
  echo "==> [macOS] Reusing Graphviz universal archive"
fi
lipo "${GRAPHVIZ_MACOS_LIB_DIR}/libgraphviz_api.a" -verify_arch arm64 x86_64
lipo "${GRAPHVIZ_MACOS_LIB_DIR}/libgraphviz_api.dylib" -verify_arch arm64 x86_64

# Both desktop Rust targets share the same source and Graphviz archive; Cargo
# still has to compile once per architecture before lipo can merge the result.
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  echo "==> [macOS] Building Rust modules for ${target}"
  MACOSX_DEPLOYMENT_TARGET=11.0 \
  GRAPHVIZ_ANYWHERE_DIR="${GRAPHVIZ_MACOS_LIB_DIR}" \
    cargo build --release --target "${target}" \
      -p supramark-markdown-native \
      -p supramark-d2-native \
      -p supramark-mermaid-native \
      -p supramark-plantuml-native
done

mkdir -p "${MACOS_TARGET_DIR}" "${APPLE_XCFRAMEWORK_DIR}"

# Merge each pair of Rust archives and dylibs. CocoaPods consumes the dylib on
# macOS so each module keeps its Rust runtime isolated; the static archive is
# still used to add the macOS slice to the Apple XCFramework.
for module in "${APPLE_NATIVE_MODULES[@]}"; do
  IFS='|' read -r crate header_dir lib_stem xcframework_name <<< "${module}"
  static_lib_name="${lib_stem}.a"
  dylib_name="${lib_stem}.dylib"

  echo "==> [macOS] Creating universal ${static_lib_name}"
  lipo -create \
    "${REPO_ROOT}/target/aarch64-apple-darwin/release/${static_lib_name}" \
    "${REPO_ROOT}/target/x86_64-apple-darwin/release/${static_lib_name}" \
    -output "${MACOS_TARGET_DIR}/${static_lib_name}"

  echo "==> [macOS] Creating universal ${dylib_name}"
  lipo -create \
    "${REPO_ROOT}/target/aarch64-apple-darwin/release/${dylib_name}" \
    "${REPO_ROOT}/target/x86_64-apple-darwin/release/${dylib_name}" \
    -output "${MACOS_TARGET_DIR}/${dylib_name}"

  # Rust cdylibs inherit an absolute target/deps install name. Published
  # libraries must use @rpath so they remain loadable from a consumer app.
  install_name_tool -id "@rpath/${dylib_name}" "${MACOS_TARGET_DIR}/${dylib_name}"

  lipo "${MACOS_TARGET_DIR}/${static_lib_name}" -verify_arch arm64 x86_64
  lipo "${MACOS_TARGET_DIR}/${dylib_name}" -verify_arch arm64 x86_64

  ios_device_lib="${REPO_ROOT}/target/aarch64-apple-ios/release/${static_lib_name}"
  ios_sim_lib="${REPO_ROOT}/target/ios-sim-universal/release/${static_lib_name}"
  if [[ -f "${ios_device_lib}" && -f "${ios_sim_lib}" ]]; then
    "${SCRIPT_DIR}/build-ios-xcframework.sh" \
      "${crate}" \
      "${header_dir}" \
      "${static_lib_name}" \
      "${APPLE_XCFRAMEWORK_DIR}/${xcframework_name}"
  else
    echo "==> [macOS] Skipping ${xcframework_name}; optional iOS inputs are absent"
  fi
done

# Graphviz's iOS inputs live inside its output XCFramework. Copy them aside
# before rebuilding that same directory, otherwise the packager would delete
# its own source files when it refreshes the output.
GRAPHVIZ_INPUT_DIR="${REPO_ROOT}/target/graphviz-apple-inputs"
GRAPHVIZ_DEVICE_SRC="${GRAPHVIZ_ROOT}/output/ios/Graphviz.xcframework/ios-arm64/libgraphviz_api.a"
GRAPHVIZ_SIM_SRC="${GRAPHVIZ_ROOT}/output/ios/Graphviz.xcframework/ios-arm64_x86_64-simulator/libgraphviz_api.a"

if [[ -f "${GRAPHVIZ_DEVICE_SRC}" && -f "${GRAPHVIZ_SIM_SRC}" ]]; then
  mkdir -p "${GRAPHVIZ_INPUT_DIR}"
  cp "${GRAPHVIZ_DEVICE_SRC}" "${GRAPHVIZ_INPUT_DIR}/libgraphviz_api-ios.a"
  cp "${GRAPHVIZ_SIM_SRC}" "${GRAPHVIZ_INPUT_DIR}/libgraphviz_api-ios-simulator.a"

  echo "==> [macOS] Adding Graphviz to the Apple XCFramework"
  SUPRAMARK_DEVICE_LIB="${GRAPHVIZ_INPUT_DIR}/libgraphviz_api-ios.a" \
  SUPRAMARK_SIM_LIB="${GRAPHVIZ_INPUT_DIR}/libgraphviz_api-ios-simulator.a" \
  SUPRAMARK_MACOS_LIB="${GRAPHVIZ_MACOS_LIB_DIR}/libgraphviz_api.a" \
    "${SCRIPT_DIR}/build-ios-xcframework.sh" \
      graphviz-native \
      "${GRAPHVIZ_ROOT}/capi" \
      libgraphviz_api.a \
      "${GRAPHVIZ_ROOT}/output/ios/Graphviz.xcframework"
else
  echo "==> [macOS] Skipping Graphviz Apple XCFramework; optional iOS inputs are absent"
fi

# Refresh the ignored package-local binaries consumed by CocoaPods and local
# file: dependencies. prepare-native also preserves any existing Android files.
echo "==> Staging Apple XCFrameworks into React Native packages"
node crates/supramark-markdown/packages/react-native/scripts/prepare-native.js
node crates/d2-little/packages/react-native/scripts/prepare-native.js
node crates/mermaid-little/packages/react-native/scripts/prepare-native.js
node crates/plantuml-little/packages/react-native/scripts/prepare-native.js
node crates/graphviz-anywhere/packages/react-native/scripts/prepare-native.js

node scripts/verify-macos-native-packages.js

echo "==> Done: all five React Native packages contain verified macOS universal dylibs"
