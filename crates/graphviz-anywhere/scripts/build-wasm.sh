#!/usr/bin/env bash
#
# Build Graphviz WebAssembly module with Embind bindings.
#
# Architecture: compiles Graphviz 14.x libs as static archives, then links
# our `packages/web/src-cpp/main.cpp` wrapper (CGraphviz class) with
# `-lembind` so user-facing calls go through typed Embind dispatch
# instead of the wasm function-pointer table. See hpcc-js-wasm for the
# reference approach.
#
# Usage:
#   ./scripts/build-wasm.sh
#
# Environment variables:
#   BUILD_DIR   - Build directory (default: build/wasm)
#   INSTALL_DIR - Install prefix (default: packages/web/dist)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/wasm}"
# Default to packages/web/dist so the published package contains the wasm.
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/packages/web/dist}"
WEB_SRC_CPP="${PROJECT_ROOT}/packages/web/src-cpp"

log_info "Building Graphviz for WebAssembly (Embind)"
log_info "Build directory: ${BUILD_DIR}"
log_info "Install directory: ${INSTALL_DIR}"

# Check for emscripten
if ! command -v emcc &>/dev/null; then
    log_error "Emscripten compiler (emcc) not found. Please install Emscripten SDK."
    exit 1
fi

log_info "Using Emscripten: $(emcc --version | head -1)"
check_command "cmake"

# Step 1: Prepare patched source (SHARED→STATIC, strip declspecs, etc.).
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

# Step 1b: Build expat through the Emscripten toolchain so Graphviz's
# HTML-label parser (htmllex/htmlparse) can be enabled. Without expat the
# HTML label family (<<TABLE>>, <<B>bold</B>>, cluster BGCOLOR, etc.) is
# silently dropped, which breaks downstream consumers such as PlantUML
# (synthetic BGCOLOR markers, HTML-styled cluster labels).
EXPAT_SRC="${BUILD_DIR}/expat-src"
EXPAT_BUILD="${BUILD_DIR}/expat-build"
EXPAT_INSTALL="${BUILD_DIR}/expat-install"
download_expat "${EXPAT_SRC}"
CMAKE_CMD="emcmake cmake" build_expat "${EXPAT_SRC}" "${EXPAT_BUILD}" "${EXPAT_INSTALL}" \
    -DCMAKE_C_FLAGS="-O2 -fPIC"

# Step 2: Configure Graphviz with Emscripten.
log_info "Configuring Graphviz for Wasm..."
mkdir -p "${BUILD_DIR}/graphviz"

# -DGRAPHVIZ_CLI=OFF skips cmd/contrib builds that don't apply to wasm.
# -Wno-incompatible-function-pointer-types is kept for defensive compat;
# 14.x is mostly clean but the occasional stray cast still appears.
# WITH_EXPAT=ON + explicit EXPAT_INCLUDE_DIR/EXPAT_LIBRARY points Graphviz
# at the static expat we just built (Emscripten FindEXPAT otherwise looks
# at the host system).
if ! emcmake cmake -S "${GV_PATCHED}" -B "${BUILD_DIR}/graphviz" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_C_FLAGS="-O2 -fwasm-exceptions -Wno-incompatible-function-pointer-types" \
    -DCMAKE_CXX_FLAGS="-O2 -fwasm-exceptions" \
    -DCMAKE_INSTALL_PREFIX="${BUILD_DIR}/graphviz-install" \
    -DGRAPHVIZ_CLI=OFF \
    -DENABLE_LTDL=OFF \
    -DENABLE_TCL=OFF \
    -DENABLE_SWIG=OFF \
    -DENABLE_SHARP=OFF \
    -DENABLE_D=OFF \
    -DENABLE_GO=OFF \
    -DENABLE_JAVASCRIPT=OFF \
    -Denable_ltdl=OFF \
    -Dwith_smyrna=OFF \
    -Dwith_digcola=ON \
    -Dwith_ortho=ON \
    -Dwith_sfdp=ON \
    -DWITH_EXPAT=ON \
    -DEXPAT_INCLUDE_DIR="${EXPAT_INSTALL}/include" \
    -DEXPAT_LIBRARY="${EXPAT_INSTALL}/lib/libexpat.a" \
    -Dwith_zlib=OFF \
    -Dwith_pangocairo=OFF \
    -DZLIB_LIBRARY="" \
    -DZLIB_INCLUDE_DIR=""; then
    log_error "CMake configuration failed"
    exit 1
fi

# Step 3: Build Graphviz targets
log_info "Building Graphviz library targets..."
GV_TARGETS=("${GV_LIB_TARGETS[@]}")
JOBS=${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}
if ! emmake cmake --build "${BUILD_DIR}/graphviz" --parallel "$JOBS" \
    --target "${GV_TARGETS[@]}"; then
    log_error "Graphviz static-lib build failed"
    exit 1
fi

GV_INSTALL="${BUILD_DIR}/graphviz-install"
install_graphviz_headers "${GV_PATCHED}" "${BUILD_DIR}/graphviz" "${GV_INSTALL}" || true

# Step 4: Collect all static libraries
log_info "Collecting static libraries..."
GV_STATIC_LIBS=()
while IFS= read -r lib; do
    GV_STATIC_LIBS+=("$lib")
done < <(collect_static_libs "${BUILD_DIR}/graphviz" "${GV_INSTALL}" 2>/dev/null)
# Append expat so the final wasm link can resolve XML_ParserCreate etc.
# pulled in by lib/common/htmllex.c when WITH_EXPAT=ON.
if [ -f "${EXPAT_INSTALL}/lib/libexpat.a" ]; then
    GV_STATIC_LIBS+=("${EXPAT_INSTALL}/lib/libexpat.a")
fi
log_info "Found ${#GV_STATIC_LIBS[@]} static libraries"

# Step 5: Compile + link the C++ Embind wrapper into a single Wasm module.
#
# Flags mirror hpcc-js-wasm's root/package CMakeLists:
#   -lembind              → Embind runtime (auto-generates JS bindings)
#   -fwasm-exceptions     → native wasm EH, avoids the invoke trampoline
#                           that historically blew up on K&R fpcasts
#   -sMODULARIZE=1        → factory-function export (ES6 default)
#   -sEXPORT_ES6=1
#   -sEXPORT_NAME=...     → factory name; picked up by loader glue
#   -sENVIRONMENT=web,worker,node
#   -sALLOW_MEMORY_GROWTH=1
#   -sFILESYSTEM=0        → drop the emscripten FS shim we don't need
#   -sSTRICT=1            → strict emscripten mode, drops legacy JS APIs
#
# Note: EXPORTED_FUNCTIONS / EXPORTED_RUNTIME_METHODS / cwrap-style exports
# are deliberately omitted — Embind handles the surface area now.
log_info "Linking WebAssembly module (Embind)..."
mkdir -p "${INSTALL_DIR}"

EMBIND_CXX_FLAGS=(
    -O2
    -fwasm-exceptions
    -DPACKAGE_VERSION="\"${GRAPHVIZ_VERSION}\""
    -I"${GV_INSTALL}/include"
    -I"${GV_INSTALL}/include/graphviz"
    # Generated graphviz_version.h lives at the CMake build root
    -I"${BUILD_DIR}/graphviz"
    # Source-tree headers for gvc.h/gvplugin.h/cgraph.h etc. that aren't
    # installed because we didn't run `cmake --install`.
    -I"${GV_PATCHED}/lib/gvc"
    -I"${GV_PATCHED}/lib/cgraph"
    -I"${GV_PATCHED}/lib/cdt"
    -I"${GV_PATCHED}/lib/pathplan"
    -I"${GV_PATCHED}/lib/common"
)

EMBIND_LINK_FLAGS=(
    -O2
    -fwasm-exceptions
    -lembind
    --no-entry
    -sWASM=1
    -sMODULARIZE=1
    -sEXPORT_ES6=1
    -sEXPORT_NAME=VizModule
    -sENVIRONMENT=web,worker,node
    -sALLOW_MEMORY_GROWTH=1
    -sFILESYSTEM=0
    -sSTRICT=1
    -sINCOMING_MODULE_JS_API=['wasmBinary','locateFile']
    -sEXPORTED_RUNTIME_METHODS=['UTF8ToString']
)

em++ "${EMBIND_CXX_FLAGS[@]}" \
    -c "${WEB_SRC_CPP}/main.cpp" \
    -o "${BUILD_DIR}/graphvizlib_main.o"

em++ "${EMBIND_LINK_FLAGS[@]}" \
    -o "${INSTALL_DIR}/viz.js" \
    "${BUILD_DIR}/graphvizlib_main.o" \
    "${GV_STATIC_LIBS[@]}"

# Step 6: Generate TypeScript declaration stub for the Embind module.
cat > "${INSTALL_DIR}/viz.d.ts" << 'TS_EOF'
// Auto-generated ambient types for the Embind wasm module emitted by
// scripts/build-wasm.sh. Shape matches emscripten MODULARIZE=1 output.
export interface CGraphviz {
  layout(dot: string, format: string, engine: string): string;
  delete(): void;
}

export interface CGraphvizConstructor {
  new (): CGraphviz;
  new (yInvert: number, nop: number): CGraphviz;
  version(): string;
  lastError(): string;
}

export interface VizModuleInstance {
  CGraphviz: CGraphvizConstructor;
}

declare const VizModule: (config?: {
  wasmBinary?: ArrayBuffer | Uint8Array;
  locateFile?: (path: string, prefix: string) => string;
}) => Promise<VizModuleInstance>;

export default VizModule;
TS_EOF

# Step 7: Verify outputs
log_info "Verifying outputs..."
verify_output "${INSTALL_DIR}/viz.wasm" "WebAssembly module"
verify_output "${INSTALL_DIR}/viz.js" "JavaScript glue code"

WASM_SIZE=$(du -h "${INSTALL_DIR}/viz.wasm" | cut -f1)
JS_SIZE=$(du -h "${INSTALL_DIR}/viz.js" | cut -f1)
log_info "WebAssembly module size: ${WASM_SIZE}"
log_info "JavaScript glue code size: ${JS_SIZE}"
log_info "Wasm build complete: ${INSTALL_DIR}"
