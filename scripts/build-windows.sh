#!/usr/bin/env bash
#
# Build Graphviz shared library for Windows (MSVC)
#
# Requires: CMake, MSVC 2019+ (via Visual Studio or Build Tools), Git Bash/MSYS2
#
# Usage:
#   ./scripts/build-windows.sh [--arch x86_64|arm64]
#
# Environment variables:
#   BUILD_DIR   - Build directory (default: build/windows-<arch>)
#   INSTALL_DIR - Install prefix (default: output/windows-<arch>)
#
# ── arm64 notes ─────────────────────────────────────────────────────────────
# --arch arm64 targets aarch64-pc-windows-msvc.
# Prerequisites on the CI runner:
#   - Visual Studio 2022 with "MSVC v143 - VS 2022 C++ ARM64 build tools"
#     component installed (workload: Desktop development with C++).
#   - CMake generator platform "ARM64" (passed automatically below).
# The arm64 build has NOT been smoke-tested locally (no ARM64 Windows host
# available).  CI validation is required before shipping release assets.
# TODO(verify-in-ci): run a matrix job on windows-11-arm runner once
# GitHub Actions makes it GA, or use QEMU/cross-toolchain on windows-latest.
# ────────────────────────────────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

ARCH="x86_64"

while [[ $# -gt 0 ]]; do
    case $1 in
        --arch) ARCH="$2"; shift 2 ;;
        *) log_error "Unknown option: $1"; exit 1 ;;
    esac
done

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *) log_error "Unsupported architecture: ${ARCH}. Must be x86_64 or arm64."; exit 1 ;;
esac

# Map our arch name to the CMake/VS generator platform string.
# x86_64 → "x64"   arm64 → "ARM64"
case "$ARCH" in
    x86_64) CMAKE_PLATFORM="x64" ;;
    arm64)  CMAKE_PLATFORM="ARM64" ;;
esac

BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/windows-${ARCH}}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/windows-${ARCH}}"

log_info "Building Graphviz for Windows ${ARCH} (CMake platform: ${CMAKE_PLATFORM})"

check_command "cmake"

# Prepare patched source
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

# Configure Graphviz (expat disabled — see GV_CMAKE_COMMON_ARGS which sets
# WITH_EXPAT=OFF for 14.x. We build without HTML-label parsing on Windows
# to match the 12.x-era behavior of this script. Graphviz 14.x's bundled
# FindEXPAT on the windows-latest runner picks up a system expat that
# doesn't expose its include dir on the vcxproj command line, which is why
# htmllex.c fails with "Cannot open include file: 'expat.h'".)
log_info "Configuring Graphviz..."
mkdir -p "${BUILD_DIR}/graphviz"
# TODO(verify-in-ci): ARM64 generator path untested — needs windows-arm64 runner
cmake -S "${GV_PATCHED}" -B "${BUILD_DIR}/graphviz" \
    -G "Visual Studio 17 2022" -A "${CMAKE_PLATFORM}" \
    "${GV_CMAKE_COMMON_ARGS[@]}" \
    -DCMAKE_INSTALL_PREFIX="${BUILD_DIR}/graphviz-install"

log_info "Building Graphviz library targets..."
cmake --build "${BUILD_DIR}/graphviz" --config Release --parallel \
    --target "${GV_LIB_TARGETS[@]}"

GV_INSTALL="${BUILD_DIR}/graphviz-install"
install_graphviz_headers "${GV_PATCHED}" "${BUILD_DIR}/graphviz" "${GV_INSTALL}"

# Build wrapper DLL
log_info "Building graphviz_api wrapper..."
mkdir -p "${BUILD_DIR}/wrapper" "${INSTALL_DIR}/lib" "${INSTALL_DIR}/bin" "${INSTALL_DIR}/include"

cat > "${BUILD_DIR}/wrapper/CMakeLists.txt" << 'CMAKE_EOF'
cmake_minimum_required(VERSION 3.16)
project(graphviz_api C)

file(GLOB_RECURSE GV_STATIC_LIBS "${GV_BUILD_DIR}/*.lib")
# Drop CMake scratch libs (feature probes / LTO tests) — they are compiled
# with host attributes and can pull in incompatible objects at link time.
list(FILTER GV_STATIC_LIBS EXCLUDE REGEX "CMakeFiles|_CMakeLTOTest-|/CMakeScratch/|/CMakeTmp/")
file(GLOB GV_INSTALL_LIBS "${GV_INSTALL_DIR}/lib/*.lib")
list(APPEND GV_ALL_LIBS ${GV_STATIC_LIBS} ${GV_INSTALL_LIBS})

# Shared DLL — kept for consumers that prefer dynamic linking.
add_library(graphviz_api SHARED "${SRC_DIR}/graphviz_api.c")

target_include_directories(graphviz_api PRIVATE
    "${GV_INSTALL_DIR}/include"
    "${GV_INSTALL_DIR}/include/graphviz"
)

target_compile_definitions(graphviz_api PRIVATE
    GRAPHVIZ_API_EXPORTS
    PACKAGE_VERSION="${GV_VERSION}"
)

target_link_libraries(graphviz_api PRIVATE ${GV_ALL_LIBS})

install(TARGETS graphviz_api
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)
CMAKE_EOF

# TODO(verify-in-ci): ARM64 wrapper CMake path untested — needs windows-arm64 runner
cmake -S "${BUILD_DIR}/wrapper" -B "${BUILD_DIR}/wrapper/build" \
    -G "Visual Studio 17 2022" -A "${CMAKE_PLATFORM}" \
    -DSRC_DIR="${WRAPPER_SRC}" \
    -DGV_BUILD_DIR="${BUILD_DIR}/graphviz" \
    -DGV_INSTALL_DIR="${GV_INSTALL}" \
    -DGV_VERSION="${GRAPHVIZ_VERSION}" \
    -DCMAKE_INSTALL_PREFIX="${INSTALL_DIR}"

cmake --build "${BUILD_DIR}/wrapper/build" --config Release
cmake --install "${BUILD_DIR}/wrapper/build" --config Release

# ── Merged static library via lib.exe ────────────────────────────────────────
# CMake's add_library(STATIC) only archives THIS target's own .obj files;
# target_link_libraries / LINKER:/WHOLEARCHIVE is a link-time flag for
# executables/DLLs and has NO effect on static archive output.  To produce a
# truly merged static .lib we must invoke lib.exe directly.
#
# lib.exe accepts .obj files and .lib archives as inputs and extracts + merges
# all member .obj files into a single output archive — no /WHOLEARCHIVE needed.
log_info "Building merged static library (graphviz_api_static.lib) via lib.exe..."

# Locate lib.exe from the MSVC toolchain.  vswhere is the canonical locator on
# windows-latest runners; fall back to PATH lookup for custom environments.
LIBEXE=""
VSWHERE="C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
if [ -f "${VSWHERE}" ]; then
    VS_INSTALL="$("${VSWHERE}" -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>/dev/null | tr -d '\r')"
    if [ -n "${VS_INSTALL}" ]; then
        # VC tools version string lives in a single-line text file
        VC_VER_FILE="${VS_INSTALL}/VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt"
        if [ -f "${VC_VER_FILE}" ]; then
            VC_VER="$(cat "${VC_VER_FILE}" | tr -d '[:space:]')"
            case "$ARCH" in
                x86_64) HOST_SUBDIR="x64" ;;
                arm64)  HOST_SUBDIR="x64" ;;  # cross-compile: host tools are x64
            esac
            CANDIDATE="${VS_INSTALL}/VC/Tools/MSVC/${VC_VER}/bin/Host${HOST_SUBDIR}/${HOST_SUBDIR}/lib.exe"
            [ -f "${CANDIDATE}" ] && LIBEXE="${CANDIDATE}"
        fi
    fi
fi
# Fallback: rely on PATH (works when vcvarsall.bat has been sourced)
if [ -z "${LIBEXE}" ]; then
    LIBEXE="$(command -v lib.exe 2>/dev/null || command -v lib 2>/dev/null || true)"
fi
if [ -z "${LIBEXE}" ]; then
    log_error "Cannot locate lib.exe — ensure MSVC build tools are installed"
    exit 1
fi
log_info "Using lib.exe: ${LIBEXE}"

# Map our arch to lib.exe /MACHINE flag
case "$ARCH" in
    x86_64) LIB_MACHINE="X64" ;;
    arm64)  LIB_MACHINE="ARM64" ;;
esac

# Collect the wrapper's compiled .obj (produced by the DLL build above).
# MSVC places per-config objects under Release/graphviz_api.obj inside the
# CMake project build directory for the source file.
WRAPPER_OBJ=""
while IFS= read -r -d '' f; do
    WRAPPER_OBJ="$f"
    break
done < <(find "${BUILD_DIR}/wrapper/build" -name "graphviz_api.obj" -print0 2>/dev/null)
if [ -z "${WRAPPER_OBJ}" ]; then
    log_error "graphviz_api.obj not found under ${BUILD_DIR}/wrapper/build"
    exit 1
fi
log_info "Wrapper object: ${WRAPPER_OBJ}"

# Collect all Graphviz component static libs from the build tree.
# Exclude CMake internal scratch archives (LTO test, feature probes).
mapfile -d '' GV_LIBS < <(find "${BUILD_DIR}/graphviz" -name "*.lib" -print0 2>/dev/null \
    | grep -zEv 'CMakeFiles|_CMakeLTOTest-|CMakeScratch|CMakeTmp')

if [ "${#GV_LIBS[@]}" -eq 0 ]; then
    log_error "No Graphviz .lib files found under ${BUILD_DIR}/graphviz"
    exit 1
fi
log_info "Merging ${#GV_LIBS[@]} Graphviz lib(s) + wrapper obj into graphviz_api_static.lib"

STATIC_OUT="${INSTALL_DIR}/lib/graphviz_api_static.lib"
"${LIBEXE}" /NOLOGO \
    "/MACHINE:${LIB_MACHINE}" \
    "/OUT:${STATIC_OUT}" \
    "${WRAPPER_OBJ}" \
    "${GV_LIBS[@]}"
# ─────────────────────────────────────────────────────────────────────────────

cp "${WRAPPER_SRC}/graphviz_api.h" "${INSTALL_DIR}/include/"

log_info "Verifying outputs..."
verify_output "${INSTALL_DIR}/bin/graphviz_api.dll" "Wrapper DLL"
verify_output "${INSTALL_DIR}/lib/graphviz_api_static.lib" "Static library (for Rust prebuilt)"
verify_output "${INSTALL_DIR}/include/graphviz_api.h" "Wrapper header"

log_info "Windows ${ARCH} build complete: ${INSTALL_DIR}"
