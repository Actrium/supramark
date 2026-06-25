#!/usr/bin/env bash
#
# Build Graphviz shared library for Linux
#
# Builds all Graphviz components as static libraries, then links them
# together with the C ABI wrapper into a single unified libgraphviz_api.so.
#
# Usage:
#   ./scripts/build-linux.sh [--arch x86_64|aarch64]
#
# Environment variables:
#   BUILD_DIR   - Build directory (default: build/linux-<arch>)
#   INSTALL_DIR - Install prefix (default: output/linux-<arch>)
#
# ── aarch64 build notes ────────────────────────────────────────────────────
# Two supported scenarios:
#
# 1. Native aarch64 host (e.g. AWS Graviton, Ampere, Raspberry Pi 64-bit):
#    Run the script without any extra setup.  gcc/g++ must target aarch64
#    natively, which is the default on those hosts.
#    Output: output/linux-aarch64/lib/libgraphviz_api.so
#
# 2. Cross-compile from x86_64 host (e.g. standard ubuntu-latest CI runner):
#    Install the cross toolchain and sysroot before running this script:
#      apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu \
#                         binutils-aarch64-linux-gnu
#    Then export CC/CXX so CMake and the wrapper compile step pick up the
#    correct cross compilers:
#      export CC=aarch64-linux-gnu-gcc
#      export CXX=aarch64-linux-gnu-g++
#    CMake will use these via CMAKE_C_COMPILER / CMAKE_CXX_COMPILER
#    auto-detection.  No CMAKE_TOOLCHAIN_FILE is required for this simple
#    native-ABI cross because we are not using Android or iOS sysroots.
#
#    pkg-config will try to find harfbuzz/pangocairo for the host arch.
#    On a plain cross-compile runner these will be absent, so pango plugin
#    support is skipped automatically (the `pkg-config --exists pangocairo`
#    check returns false and gvplugin_pango is omitted from the build).
#    If you need pango on aarch64, install the cross-compiled dev packages:
#      apt-get install -y libpango1.0-dev:arm64
#    and point PKG_CONFIG_PATH / PKG_CONFIG_LIBDIR at the arm64 sysroot.
#
# Output layout (both scenarios):
#   output/linux-aarch64/lib/libgraphviz_api.so
#   output/linux-aarch64/lib/libgraphviz_api.a   (static archive — for Rust prebuilt)
#   output/linux-aarch64/include/graphviz_api.h
# ────────────────────────────────────────────────────────────────────────────
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

ARCH="${ARCH:-$(uname -m)}"

while [[ $# -gt 0 ]]; do
    case $1 in
        --arch) ARCH="$2"; shift 2 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) log_error "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/linux-${ARCH}}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/linux-${ARCH}}"

log_info "Building Graphviz for Linux ${ARCH}"
log_info "Build directory: ${BUILD_DIR}"
log_info "Install directory: ${INSTALL_DIR}"

check_build_deps
for dep in bison flex; do
    check_command "$dep"
done

# Detect extra include paths for harfbuzz (needed by pango headers on some distros)
HB_CFLAGS=""
if command -v pkg-config &>/dev/null; then
    HB_CFLAGS="$(pkg-config --cflags-only-I harfbuzz 2>/dev/null || true)"
fi

# Step 1: Prepare patched source
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

# Step 2: Configure and build Graphviz (all static)
#
# Linux build hosts (ubuntu-latest in CI) have libexpat + libz available
# via apt, so we enable HTML-label support (requires expat) and DEFLATE
# compression (requires zlib). Downstream consumers that rely on
# record/HTML labels (e.g. plantuml-little) need these.
log_info "Configuring Graphviz..."
mkdir -p "${BUILD_DIR}/graphviz"
cmake -S "${GV_PATCHED}" -B "${BUILD_DIR}/graphviz" \
    "${GV_CMAKE_COMMON_ARGS[@]}" \
    -DWITH_EXPAT=ON \
    -DWITH_ZLIB=ON \
    "-DCMAKE_C_FLAGS=-O2 -fPIC ${HB_CFLAGS}" \
    -DCMAKE_INSTALL_PREFIX="${BUILD_DIR}/graphviz-install"

log_info "Building Graphviz library targets..."
GV_TARGETS=("${GV_LIB_TARGETS[@]}")
# Add pango plugin if available
if pkg-config --exists pangocairo 2>/dev/null; then
    GV_TARGETS+=("gvplugin_pango")
fi
cmake --build "${BUILD_DIR}/graphviz" --parallel "$(nproc)" \
    --target "${GV_TARGETS[@]}"

GV_INSTALL="${BUILD_DIR}/graphviz-install"
install_graphviz_headers "${GV_PATCHED}" "${BUILD_DIR}/graphviz" "${GV_INSTALL}"

# Step 3: Collect all static libraries and build unified .so
log_info "Collecting static libraries..."
GV_STATIC_LIBS=()
while IFS= read -r lib; do
    GV_STATIC_LIBS+=("$lib")
done < <(collect_static_libs "${BUILD_DIR}/graphviz" "${GV_INSTALL}")
log_info "Found ${#GV_STATIC_LIBS[@]} static libraries"

log_info "Building unified libgraphviz_api.so..."
mkdir -p "${INSTALL_DIR}/lib" "${INSTALL_DIR}/include"

# Respect CC/CXX for cross-compilation (e.g. CC=aarch64-linux-gnu-gcc).
# Falls back to gcc/g++ on native builds.
${CC:-gcc} -c -fPIC -O2 \
    -DPACKAGE_VERSION="\"${GRAPHVIZ_VERSION}\"" \
    -I"${GV_INSTALL}/include" \
    -I"${GV_INSTALL}/include/graphviz" \
    -o "${BUILD_DIR}/graphviz_api.o" \
    "${WRAPPER_SRC}/graphviz_api.c"

# System libraries. WITH_EXPAT=OFF / WITH_ZLIB=OFF in GV_CMAKE_COMMON_ARGS
# means the static libs shouldn't reference libexpat / libz. Keep them in
# SYS_LIBS defensively on Linux where apt pulls them in anyway — the linker
# will just resolve them. Drop only if the link actually fails.
SYS_LIBS=(-lm -lz -lexpat)
if pkg-config --exists pangocairo 2>/dev/null; then
    while IFS= read -r flag; do
        [[ -n "$flag" ]] && SYS_LIBS+=("$flag")
    done < <(pkg-config --libs pangocairo | tr ' ' '\n')
fi

# Graphviz 14.x introduced first-class C++ libraries (vpsc, neatogen
# layout cost models). Linking the unified .so with `gcc` leaves libstdc++
# symbols unresolved at runtime (e.g. dlopen fails with
# `undefined symbol: _ZTVN10__cxxabiv117__class_type_infoE`). Use `g++` so
# the C++ runtime is pulled in, matching what build-macos.sh does with
# `clang++`.
${CXX:-g++} -shared -o "${INSTALL_DIR}/lib/libgraphviz_api.so" \
    "${BUILD_DIR}/graphviz_api.o" \
    -Wl,--whole-archive \
    "${GV_STATIC_LIBS[@]}" \
    -Wl,--no-whole-archive \
    "${SYS_LIBS[@]}"

cp "${WRAPPER_SRC}/graphviz_api.h" "${INSTALL_DIR}/include/"

# Step 4: Also produce a static archive (libgraphviz_api.a)
# This is what packages/rust/build.rs + Cargo.toml (links = "graphviz_api",
# cargo:rustc-link-lib=static=graphviz_api) expect in the prebuilt layout.
# The shared .so is kept for consumers that prefer dynamic linking.
log_info "Building static archive libgraphviz_api.a..."
AR="${AR:-ar}"
# Respect cross-compilation AR (e.g. AR=aarch64-linux-gnu-ar).
#
# `ar rcs out.a wrapper.o libfoo.a` stores each libfoo.a as an opaque archive
# *member* rather than merging its objects, so the Graphviz symbols never end
# up in the final archive (the linker then reports them undefined). Use an MRI
# script: ADDLIB pulls every object out of each component static library, so
# the result is a single self-contained archive — mirroring what the .so gets
# via --whole-archive.
ar_mri="${BUILD_DIR}/libgraphviz_api.mri"
{
    echo "CREATE ${INSTALL_DIR}/lib/libgraphviz_api.a"
    echo "ADDMOD ${BUILD_DIR}/graphviz_api.o"
    for _lib in "${GV_STATIC_LIBS[@]}"; do
        echo "ADDLIB ${_lib}"
    done
    echo "SAVE"
    echo "END"
} > "${ar_mri}"
"${AR}" -M < "${ar_mri}"

# Step 5: Verify
log_info "Verifying outputs..."
verify_output "${INSTALL_DIR}/lib/libgraphviz_api.so" "Unified shared library"
verify_output "${INSTALL_DIR}/lib/libgraphviz_api.a" "Unified static archive"
verify_output "${INSTALL_DIR}/include/graphviz_api.h" "Wrapper header"

log_info "Library size (.so): $(du -h "${INSTALL_DIR}/lib/libgraphviz_api.so" | cut -f1)"
log_info "Library size (.a):  $(du -h "${INSTALL_DIR}/lib/libgraphviz_api.a"  | cut -f1)"
log_info "Linux ${ARCH} build complete: ${INSTALL_DIR}"
