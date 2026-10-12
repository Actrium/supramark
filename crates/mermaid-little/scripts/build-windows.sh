#!/usr/bin/env bash
#
# Build supramark-mermaid-native for Windows (MSVC).
#
# This crate is pure Rust — no external C dependencies.
# Cross-compile the cdylib target to produce supramark_mermaid_native.dll,
# then stage the DLL + import lib + C header into output/windows-x86_64/.
#
# Requires:
#   - Rust target (x86_64 default; aarch64 for --arch arm64):
#       rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
#   - MSVC build tools (Visual Studio 2019+ or Build Tools)
#   - Run on Windows (Git Bash / MSYS2) or CI windows-latest runner.
#
# Usage: ./scripts/build-windows.sh [--arch x86_64|arm64]

set -euo pipefail

CRATE_NAME="supramark-mermaid-native"
DLL_NAME="supramark_mermaid_native.dll"
HEADER_NAME="supramark_mermaid.h"

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
    x86_64|amd64) ARCH="x86_64"; RUST_ARCH="x86_64" ;;
    arm64|aarch64) ARCH="arm64"; RUST_ARCH="aarch64" ;;
    *) echo "Unsupported architecture: ${ARCH}"; exit 1 ;;
esac

# rustc has no arm64-pc-windows-msvc triple — the Windows-on-ARM target is
# aarch64-pc-windows-msvc (same mapping as graphviz-anywhere build_helpers.rs).
# Staged output dirs keep the windows-${ARCH} naming.
RUST_TARGET="${RUST_ARCH}-pc-windows-msvc"
BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/windows-${ARCH}}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/windows-${ARCH}}"

echo "[1/4] Checking Rust target ${RUST_TARGET}..."
if ! rustup target list --installed | grep -q "${RUST_TARGET}"; then
    rustup target add "${RUST_TARGET}"
fi

echo "[2/4] Building ${CRATE_NAME} for ${RUST_TARGET}..."
cd "${CRATE_DIR}"
export CARGO_TARGET_DIR="${CRATE_DIR}/target"
cargo build --release --target "${RUST_TARGET}"

DLL_SRC="${CRATE_DIR}/target/${RUST_TARGET}/release/${DLL_NAME}"
if [[ ! -f "${DLL_SRC}" ]]; then
    echo "ERROR: Build output not found: ${DLL_SRC}"
    exit 1
fi

echo "[3/4] Staging artifacts to ${INSTALL_DIR}..."
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
    echo "[4/4] Generating import library..."
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
    supramark_mermaid_render
    supramark_mermaid_render_with_id
    supramark_mermaid_free
    supramark_mermaid_version
DEF_EOF
    LIB_OUT="$(cygpath -w "${INSTALL_DIR}/lib/${LIB_BASE}.lib" 2>/dev/null || echo "${INSTALL_DIR}/lib/${LIB_BASE}.lib")"
    DEF_WIN="$(cygpath -w "${DEF_FILE}" 2>/dev/null || echo "${DEF_FILE}")"
    MSYS2_ARG_CONV_EXCL='*' "${LIBEXE}" /NOLOGO \
        "/MACHINE:${LIB_MACHINE}" \
        "/DEF:${DEF_WIN}" \
        "/OUT:${LIB_OUT}" || echo "WARNING: import lib generation failed"
else
    echo "[4/4] WARNING: lib.exe not found; skipping import library."
fi

echo ""
echo "Windows ${ARCH} build complete: ${INSTALL_DIR}"
