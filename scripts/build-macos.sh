#!/usr/bin/env bash
#
# Build Graphviz shared library for macOS (universal binary: arm64 + x86_64)
#
# Usage:
#   ./scripts/build-macos.sh [--arch arm64|x86_64|universal]
#
# Environment variables:
#   BUILD_DIR   - Build directory (default: build/macos-<arch>)
#   INSTALL_DIR - Install prefix (default: output/macos-<arch>)
#
# Outputs:
#   output/macos-<arch>/lib/libgraphviz_api.dylib  (shared — existing)
#   output/macos-<arch>/lib/libgraphviz_api.a       (static — for Rust prebuilt)
#   output/macos-<arch>/include/graphviz_api.h
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

ARCH="${ARCH:-universal}"

while [[ $# -gt 0 ]]; do
    case $1 in
        --arch) ARCH="$2"; shift 2 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/macos-${ARCH}}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/macos-${ARCH}}"

log_info "Building Graphviz for macOS ${ARCH}"

check_build_deps

# Prepare patched source
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

build_single_arch() {
    local arch="$1"
    local build_dir="${BUILD_DIR}/${arch}"
    local gv_install="${build_dir}/graphviz-install"

    log_info "Building for macOS ${arch}..."
    mkdir -p "${build_dir}/graphviz"

    # macOS ships both libexpat and libz system-wide, so we can enable
    # HTML-label support (requires expat) and DEFLATE (requires zlib)
    # on this platform without taking on any runtime dependency beyond
    # what is already in /usr/lib. Downstream consumers that rely on
    # record/HTML labels (e.g. plantuml-little) need this.
    cmake -S "${GV_PATCHED}" -B "${build_dir}/graphviz" \
        "${GV_CMAKE_COMMON_ARGS[@]}" \
        -DWITH_EXPAT=ON \
        -DWITH_ZLIB=ON \
        "-DCMAKE_C_FLAGS=-O2 -fPIC -Wno-incompatible-function-pointer-types" \
        -DCMAKE_OSX_ARCHITECTURES="${arch}" \
        -DCMAKE_OSX_DEPLOYMENT_TARGET="10.15" \
        -DCMAKE_INSTALL_PREFIX="${gv_install}"

    cmake --build "${build_dir}/graphviz" --parallel "$(sysctl -n hw.ncpu)" \
        --target "${GV_LIB_TARGETS[@]}"

    install_graphviz_headers "${GV_PATCHED}" "${build_dir}/graphviz" "${gv_install}"

    # Collect static libs and build unified dylib
    local libs=()
    while IFS= read -r lib; do
        libs+=("$lib")
    done < <(collect_static_libs "${build_dir}/graphviz" "${gv_install}")

    clang -c -fPIC -O2 \
        -arch "${arch}" \
        -mmacosx-version-min=10.15 \
        -DPACKAGE_VERSION="\"${GRAPHVIZ_VERSION}\"" \
        -I"${gv_install}/include" \
        -I"${gv_install}/include/graphviz" \
        -o "${build_dir}/graphviz_api.o" \
        "${WRAPPER_SRC}/graphviz_api.c"

    mkdir -p "${build_dir}/out"
    # Graphviz 14.x introduced first-class C++ libraries (vpsc,
    # neatogen layout cost models). Linking with `clang` leaves libc++
    # symbols unresolved; use `clang++` to pull the C++ runtime in.
    # WITH_EXPAT=OFF / WITH_ZLIB=OFF in GV_CMAKE_COMMON_ARGS means the
    # static libs shouldn't reference libexpat / libz. Keep the flags
    # here defensively — macOS ships both, and the linker simply ignores
    # unused libraries.
    clang++ -shared -arch "${arch}" -mmacosx-version-min=10.15 \
        -install_name "@rpath/libgraphviz_api.dylib" \
        -o "${build_dir}/out/libgraphviz_api.dylib" \
        "${build_dir}/graphviz_api.o" \
        -Wl,-all_load \
        "${libs[@]}" \
        -lm -lz -lexpat

    # Also produce a static archive for packages/rust/prebuilt.
    # libtool -static handles fat-archive merging correctly on macOS;
    # plain `ar` cannot merge multi-arch .a files.
    log_info "Building static archive for macOS ${arch}..."
    libtool -static \
        -o "${build_dir}/out/libgraphviz_api.a" \
        "${build_dir}/graphviz_api.o" \
        "${libs[@]}"
}

if [ "$ARCH" = "universal" ]; then
    build_single_arch "arm64"
    build_single_arch "x86_64"

    log_info "Creating universal binary..."
    mkdir -p "${INSTALL_DIR}/lib" "${INSTALL_DIR}/include"
    lipo -create \
        "${BUILD_DIR}/arm64/out/libgraphviz_api.dylib" \
        "${BUILD_DIR}/x86_64/out/libgraphviz_api.dylib" \
        -output "${INSTALL_DIR}/lib/libgraphviz_api.dylib"

    # Merge per-arch static archives into a universal fat .a.
    # libtool -static on macOS handles multi-arch merging natively.
    log_info "Creating universal static archive..."
    libtool -static \
        -o "${INSTALL_DIR}/lib/libgraphviz_api.a" \
        "${BUILD_DIR}/arm64/out/libgraphviz_api.a" \
        "${BUILD_DIR}/x86_64/out/libgraphviz_api.a"

    cp "${WRAPPER_SRC}/graphviz_api.h" "${INSTALL_DIR}/include/"
else
    build_single_arch "$ARCH"
    mkdir -p "${INSTALL_DIR}/lib" "${INSTALL_DIR}/include"
    cp "${BUILD_DIR}/${ARCH}/out/libgraphviz_api.dylib" "${INSTALL_DIR}/lib/"
    cp "${BUILD_DIR}/${ARCH}/out/libgraphviz_api.a" "${INSTALL_DIR}/lib/"
    cp "${WRAPPER_SRC}/graphviz_api.h" "${INSTALL_DIR}/include/"
fi

log_info "Verifying outputs..."
verify_output "${INSTALL_DIR}/lib/libgraphviz_api.dylib" "Unified shared library"
verify_output "${INSTALL_DIR}/lib/libgraphviz_api.a" "Unified static archive"
verify_output "${INSTALL_DIR}/include/graphviz_api.h" "Wrapper header"

log_info "macOS ${ARCH} build complete: ${INSTALL_DIR}"
