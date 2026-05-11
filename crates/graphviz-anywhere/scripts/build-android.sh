#!/usr/bin/env bash
#
# Build Graphviz shared library for Android
#
# Builds a unified libgraphviz_api.so per ABI.
#
# Usage:
#   ./scripts/build-android.sh [--abi arm64-v8a|armeabi-v7a|x86_64|x86]
#
# Pass --abi multiple times to build a subset, e.g.:
#   ./scripts/build-android.sh --abi arm64-v8a --abi x86_64
#
# Default builds all four ABIs listed in packages/react-native/android/build.gradle:
#   arm64-v8a  armeabi-v7a  x86_64  x86
#
# Environment variables:
#   ANDROID_NDK_HOME - Path to Android NDK (required)
#   BUILD_DIR        - Build directory (default: build/android)
#   INSTALL_DIR      - Install prefix (default: output/android)
#   ANDROID_API      - Minimum API level (default: 23)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

ANDROID_API="${ANDROID_API:-23}"
TARGET_ABIS=()

while [[ $# -gt 0 ]]; do
    case $1 in
        --abi) TARGET_ABIS+=("$2"); shift 2 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

if [ ${#TARGET_ABIS[@]} -eq 0 ]; then
    # Default: all four ABIs declared in packages/react-native/android/build.gradle.
    # x86 (i686) is required for Android emulator on Intel hosts.
    TARGET_ABIS=("arm64-v8a" "armeabi-v7a" "x86_64" "x86")
fi

# Find NDK
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    for ndk_path in \
        "${HOME}/Android/Sdk/ndk"/* \
        "${HOME}/Library/Android/sdk/ndk"/* \
        "${ANDROID_HOME:-/nonexistent}/ndk"/*; do
        if [ -f "${ndk_path}/build/cmake/android.toolchain.cmake" ]; then
            ANDROID_NDK_HOME="$ndk_path"
            break
        fi
    done
fi

if [ -z "${ANDROID_NDK_HOME:-}" ] || [ ! -d "${ANDROID_NDK_HOME}" ]; then
    log_error "ANDROID_NDK_HOME is not set or invalid"
    exit 1
fi

NDK_TOOLCHAIN="${ANDROID_NDK_HOME}/build/cmake/android.toolchain.cmake"

BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/android}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/android}"

log_info "Building Graphviz for Android (NDK: ${ANDROID_NDK_HOME})"
log_info "Target ABIs: ${TARGET_ABIS[*]}"

check_build_deps

# Prepare patched source (shared across ABIs)
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

# NOTE: expat is no longer required — GV_CMAKE_COMMON_ARGS sets
# WITH_EXPAT=OFF for Graphviz 14.x. HTML labels are disabled on Android
# to match the other native platforms. If re-enabled later, restore the
# download_expat/build_expat calls and the -DEXPAT_* cmake forwards.

build_android_abi() {
    local abi="$1"
    local build_dir="${BUILD_DIR}/${abi}"
    local gv_install="${build_dir}/graphviz-install"
    local install_dir="${INSTALL_DIR}/${abi}"

    log_info "Building for Android ${abi}..."

    mkdir -p "${build_dir}/graphviz"
    cmake -S "${GV_PATCHED}" -B "${build_dir}/graphviz" \
        -DCMAKE_TOOLCHAIN_FILE="${NDK_TOOLCHAIN}" \
        -DANDROID_ABI="${abi}" \
        -DANDROID_NATIVE_API_LEVEL="${ANDROID_API}" \
        -DANDROID_STL=c++_shared \
        "${GV_CMAKE_COMMON_ARGS[@]}" \
        "-DCMAKE_C_FLAGS=-O2 -fPIC" \
        -DCMAKE_INSTALL_PREFIX="${gv_install}"

    # No pango on Android
    cmake --build "${build_dir}/graphviz" \
        --parallel "$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)" \
        --target "${GV_LIB_TARGETS[@]}"

    install_graphviz_headers "${GV_PATCHED}" "${build_dir}/graphviz" "${gv_install}"

    # Collect static libs
    local libs=()
    while IFS= read -r lib; do
        libs+=("$lib")
    done < <(collect_static_libs "${build_dir}/graphviz" "${gv_install}")

    # Determine NDK clang. BSD find (macOS) lacks `-printf`, so derive the
    # host-tag via basename of the single prebuilt directory.
    local cc
    local host_tag
    host_tag=$(basename "$(find "${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/" -mindepth 1 -maxdepth 1 -type d | head -1)")
    cc="${ANDROID_NDK_HOME}/toolchains/llvm/prebuilt/${host_tag}/bin/clang"

    local target_flag=""
    case "$abi" in
        arm64-v8a)    target_flag="--target=aarch64-linux-android${ANDROID_API}" ;;
        armeabi-v7a)  target_flag="--target=armv7a-linux-androideabi${ANDROID_API}" ;;
        x86_64)       target_flag="--target=x86_64-linux-android${ANDROID_API}" ;;
        x86)          target_flag="--target=i686-linux-android${ANDROID_API}" ;;
        *)            log_error "Unknown ABI: ${abi}"; exit 1 ;;
    esac

    "${cc}" -c -fPIC -O2 \
        "${target_flag}" \
        -DPACKAGE_VERSION="\"${GRAPHVIZ_VERSION}\"" \
        -I"${gv_install}/include" \
        -I"${gv_install}/include/graphviz" \
        -o "${build_dir}/graphviz_api.o" \
        "${WRAPPER_SRC}/graphviz_api.c"

    mkdir -p "${install_dir}/lib" "${install_dir}/include"
    # WITH_EXPAT=OFF / WITH_ZLIB=OFF → no -lexpat / -lz needed.
    "${cc}" -shared "${target_flag}" \
        -o "${install_dir}/lib/libgraphviz_api.so" \
        "${build_dir}/graphviz_api.o" \
        -Wl,--whole-archive \
        "${libs[@]}" \
        -Wl,--no-whole-archive \
        -lm

    cp "${WRAPPER_SRC}/graphviz_api.h" "${install_dir}/include/"

    verify_output "${install_dir}/lib/libgraphviz_api.so" "${abi} unified library"
}

for abi in "${TARGET_ABIS[@]}"; do
    build_android_abi "$abi"
done

log_info "Android build complete: ${INSTALL_DIR}"
