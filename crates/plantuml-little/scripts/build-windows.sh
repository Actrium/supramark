#!/usr/bin/env bash
#
# Build supramark-plantuml-native for Windows (MSVC).
#
# supramark-plantuml-native depends on graphviz-anywhere (native Graphviz C
# bindings), so this is NOT a pure-Rust build: the Graphviz static library
# (graphviz_api_static.lib) must exist before cargo runs. This script builds it
# first via crates/graphviz-anywhere/scripts/build-windows.sh unless
# GRAPHVIZ_ANYWHERE_DIR already points at a prebuilt output/windows-<arch> tree.
#
# Cross-compile the cdylib target to produce supramark_plantuml_native.dll,
# then stage the DLL + import lib + C header into output/windows-<arch>/.
#
# Requires:
#   - Rust target x86_64-pc-windows-msvc: rustup target add x86_64-pc-windows-msvc
#   - MSVC build tools (Visual Studio 2019+ or Build Tools)
#   - CMake + a C/C++ compiler (for the Graphviz native build)
#   - Run on Windows (Git Bash / MSYS2) or CI windows-latest runner.
#
# Usage: ./scripts/build-windows.sh [--arch x86_64|arm64]

set -euo pipefail

CRATE_NAME="supramark-plantuml-native"
DLL_NAME="supramark_plantuml_native.dll"
HEADER_NAME="supramark_plantuml.h"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CRATE_DIR="${PROJECT_ROOT}/packages/native"

ARCH="x86_64"
while [[ $# -gt 0 ]]; do
    case $1 in
        --arch) ARCH="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *) echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
esac

# Rust names the MSVC triples x86_64-/aarch64-pc-windows-msvc; the arm64 alias
# does NOT map to "arm64-pc-windows-msvc", so map it explicitly. Keep ARCH as
# "arm64" for directory names (matches graphviz-anywhere's output/windows-arm64).
case "$ARCH" in
    x86_64) RUST_TARGET="x86_64-pc-windows-msvc" ;;
    arm64)  RUST_TARGET="aarch64-pc-windows-msvc" ;;
esac
BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/windows-${ARCH}}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/windows-${ARCH}}"

echo "[1/5] Checking Rust target ${RUST_TARGET}..."
if ! rustup target list --installed | grep -q "${RUST_TARGET}"; then
    rustup target add "${RUST_TARGET}"
fi

# supramark-plantuml-native links graphviz-anywhere, whose build.rs locates the
# Graphviz static library through GRAPHVIZ_ANYWHERE_DIR (searched as
# <dir>/lib/graphviz_api_static.lib) and aborts the build when it is missing.
# Resolve it up front so a clean tree builds without manual intervention.
REPO_ROOT="$(cd "${PROJECT_ROOT}/../.." && pwd)"
GRAPHVIZ_CRATE="${REPO_ROOT}/crates/graphviz-anywhere"
GRAPHVIZ_OUTPUT="${GRAPHVIZ_CRATE}/output/windows-${ARCH}"

echo "[2/5] Resolving Graphviz native library..."
if [[ -n "${GRAPHVIZ_ANYWHERE_DIR:-}" && -f "${GRAPHVIZ_ANYWHERE_DIR}/lib/graphviz_api_static.lib" ]]; then
    echo "      Using prebuilt Graphviz: ${GRAPHVIZ_ANYWHERE_DIR}"
elif [[ -f "${GRAPHVIZ_OUTPUT}/lib/graphviz_api_static.lib" ]]; then
    export GRAPHVIZ_ANYWHERE_DIR="${GRAPHVIZ_OUTPUT}"
    echo "      Using prebuilt Graphviz: ${GRAPHVIZ_ANYWHERE_DIR}"
else
    if [[ ! -f "${GRAPHVIZ_CRATE}/scripts/build-windows.sh" ]]; then
        echo "ERROR: graphviz-anywhere crate not found at ${GRAPHVIZ_CRATE}" >&2
        exit 1
    fi
    # prepare_graphviz_source() copies from the graphviz/ submodule; make sure
    # it is checked out before invoking the Graphviz build.
    if [[ ! -f "${GRAPHVIZ_CRATE}/graphviz/CMakeLists.txt" ]]; then
        echo "      Initializing graphviz submodule..."
        git -C "${REPO_ROOT}" submodule update --init crates/graphviz-anywhere/graphviz
    fi
    echo "      Building Graphviz for windows-${ARCH} (this can take a while)..."
    "${GRAPHVIZ_CRATE}/scripts/build-windows.sh" --arch "${ARCH}"
    export GRAPHVIZ_ANYWHERE_DIR="${GRAPHVIZ_OUTPUT}"
fi

echo "[3/5] Building ${CRATE_NAME} for ${RUST_TARGET}..."
cd "${CRATE_DIR}"
export CARGO_TARGET_DIR="${CRATE_DIR}/target"
cargo build --release --target "${RUST_TARGET}"

DLL_SRC="${CRATE_DIR}/target/${RUST_TARGET}/release/${DLL_NAME}"
if [[ ! -f "${DLL_SRC}" ]]; then
    echo "ERROR: Build output not found: ${DLL_SRC}"
    exit 1
fi

echo "[4/5] Staging artifacts to ${INSTALL_DIR}..."
mkdir -p "${INSTALL_DIR}/bin" "${INSTALL_DIR}/lib" "${INSTALL_DIR}/include"
cp "${DLL_SRC}" "${INSTALL_DIR}/bin/"

HEADER_SRC="${CRATE_DIR}/include/${HEADER_NAME}"
if [[ -f "${HEADER_SRC}" ]]; then
    cp "${HEADER_SRC}" "${INSTALL_DIR}/include/"
fi

# Generate import library (.lib) from the DLL using lib.exe.
LIBEXE=""
VSWHERE="C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
if [[ -f "${VSWHERE}" ]]; then
    VS_INSTALL="$("${VSWHERE}" -latest -products '*' -property installationPath 2>/dev/null | tr -d '\r')"
    if [[ -n "${VS_INSTALL}" ]]; then
        VC_VER_FILE="${VS_INSTALL}/VC/Auxiliary/Build/Microsoft.VCToolsVersion.default.txt"
        if [[ -f "${VC_VER_FILE}" ]]; then
            VC_VER="$(cat "${VC_VER_FILE}" | tr -d '[:space:]')"
            case "$ARCH" in
                x86_64) HOST_TOOL_DIRS=("Hostx64/x64" "Hostx86/x64") ;;
                arm64)  HOST_TOOL_DIRS=("Hostarm64/arm64" "Hostx64/arm64") ;;
            esac
            for host_dir in "${HOST_TOOL_DIRS[@]}"; do
                CANDIDATE="${VS_INSTALL}/VC/Tools/MSVC/${VC_VER}/bin/${host_dir}/lib.exe"
                if [[ -f "${CANDIDATE}" ]]; then
                    LIBEXE="${CANDIDATE}"
                    break
                fi
            done
        fi
    fi
fi
[[ -z "${LIBEXE}" ]] && LIBEXE="$(command -v lib.exe 2>/dev/null || command -v lib 2>/dev/null || true)"

if [[ -n "${LIBEXE}" ]]; then
    echo "[5/5] Generating import library..."
    case "$ARCH" in
        x86_64) LIB_MACHINE="X64" ;;
        arm64)  LIB_MACHINE="ARM64" ;;
    esac
    DEF_FILE="${BUILD_DIR}/${DLL_NAME%.dll}.def"
    mkdir -p "${BUILD_DIR}"
    LIB_BASE="${DLL_NAME%.dll}"
    cat > "${DEF_FILE}" << DEF_EOF
LIBRARY ${LIB_BASE}
EXPORTS
    supramark_plantuml_render
    supramark_plantuml_free
    supramark_plantuml_version
    supramark_install_metrics_callback
DEF_EOF
    LIB_OUT="$(cygpath -w "${INSTALL_DIR}/lib/${LIB_BASE}.lib" 2>/dev/null || echo "${INSTALL_DIR}/lib/${LIB_BASE}.lib")"
    DEF_WIN="$(cygpath -w "${DEF_FILE}" 2>/dev/null || echo "${DEF_FILE}")"
    MSYS2_ARG_CONV_EXCL='*' "${LIBEXE}" /NOLOGO \
        "/MACHINE:${LIB_MACHINE}" \
        "/DEF:${DEF_WIN}" \
        "/OUT:${LIB_OUT}" || echo "WARNING: import lib generation failed"
else
    echo "[5/5] WARNING: lib.exe not found; skipping import library."
fi

echo ""
echo "Windows ${ARCH} build complete: ${INSTALL_DIR}"
