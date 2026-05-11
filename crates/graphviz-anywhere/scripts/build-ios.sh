#!/usr/bin/env bash
#
# Build Graphviz.xcframework for iOS
#
# Produces a unified static XCFramework containing all Graphviz functionality
# for arm64 (device), arm64 (simulator), and x86_64 (simulator, Intel Mac).
# The two simulator slices are lipo'd into a fat .a before being passed to
# xcodebuild -create-xcframework.
#
# Usage:
#   ./scripts/build-ios.sh
#
# Environment variables:
#   BUILD_DIR       - Build directory (default: build/ios)
#   INSTALL_DIR     - Install prefix (default: output/ios)
#   IOS_MIN_VERSION - Minimum iOS version (default: 15.1)
#
# CI also packages per-slice tarballs — see .github/workflows/build.yml
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

IOS_MIN_VERSION="${IOS_MIN_VERSION:-15.1}"
BUILD_DIR="${BUILD_DIR:-${PROJECT_ROOT}/build/ios}"
INSTALL_DIR="${INSTALL_DIR:-${PROJECT_ROOT}/output/ios}"

log_info "Building Graphviz for iOS (min version: ${IOS_MIN_VERSION})"

check_build_deps
check_command "xcrun"
check_command "xcodebuild"

# Prepare patched source
mkdir -p "${BUILD_DIR}"
GV_PATCHED="${BUILD_DIR}/graphviz-src"
prepare_graphviz_source "${GV_PATCHED}"

get_sdk_path() {
    xcrun --sdk "$1" --show-sdk-path
}

build_ios_arch() {
    local arch="$1"
    local sdk="$2"
    local sdk_path
    sdk_path=$(get_sdk_path "$sdk")
    local build_dir="${BUILD_DIR}/${sdk}-${arch}"
    local gv_install="${build_dir}/graphviz-install"

    # Use target triple to ensure correct platform tagging (device vs simulator)
    local target_triple="${arch}-apple-ios${IOS_MIN_VERSION}"
    if [[ "$sdk" == "iphonesimulator" ]]; then
        target_triple="${arch}-apple-ios${IOS_MIN_VERSION}-simulator"
    fi

    log_info "Building for iOS ${sdk} ${arch} (target: ${target_triple})..."

    mkdir -p "${build_dir}/graphviz"
    # Graphviz 14.x adds first-class C++ libraries (vpsc, neatogen cost
    # models, sfdpgen). Without explicit CXX flags, CMake's iOS defaults
    # can emit LLVM bitcode objects (magic 0xb17c0de) that
    # `xcodebuild -create-xcframework` refuses because it can't read the
    # architecture from a bitcode-only archive. Pin -fno-lto / -O2 and
    # disable embed-bitcode to force conventional Mach-O objects.
    cmake -S "${GV_PATCHED}" -B "${build_dir}/graphviz" \
        -DCMAKE_SYSTEM_NAME=iOS \
        -DCMAKE_OSX_ARCHITECTURES="${arch}" \
        -DCMAKE_OSX_SYSROOT="${sdk_path}" \
        -DCMAKE_OSX_DEPLOYMENT_TARGET="${IOS_MIN_VERSION}" \
        "${GV_CMAKE_COMMON_ARGS[@]}" \
        "-DCMAKE_C_FLAGS=-target ${target_triple} -O2 -fPIC -fno-lto -fembed-bitcode=off -Wno-incompatible-function-pointer-types" \
        "-DCMAKE_CXX_FLAGS=-target ${target_triple} -O2 -fPIC -fno-lto -fembed-bitcode=off" \
        -DCMAKE_XCODE_ATTRIBUTE_ENABLE_BITCODE=NO \
        -DCMAKE_INSTALL_PREFIX="${gv_install}"

    # Build only library targets (skip pango — not available on iOS)
    cmake --build "${build_dir}/graphviz" --parallel "$(sysctl -n hw.ncpu)" \
        --target "${GV_LIB_TARGETS[@]}"

    install_graphviz_headers "${GV_PATCHED}" "${build_dir}/graphviz" "${gv_install}"

    # Compile wrapper
    xcrun -sdk "${sdk}" clang -c -O2 \
        -target "${target_triple}" \
        -isysroot "${sdk_path}" \
        -DPACKAGE_VERSION="\"${GRAPHVIZ_VERSION}\"" \
        -I"${gv_install}/include" \
        -I"${gv_install}/include/graphviz" \
        -o "${build_dir}/graphviz_api.o" \
        "${WRAPPER_SRC}/graphviz_api.c"

    # Merge all .a and wrapper .o into single archive
    local libs=()
    while IFS= read -r lib; do
        libs+=("$lib")
    done < <(collect_static_libs "${build_dir}/graphviz" "${gv_install}")

    local tmpdir="${build_dir}/merge_objs"
    rm -rf "${tmpdir}"
    mkdir -p "${tmpdir}"
    for lib in "${libs[@]}"; do
        local libname
        libname=$(basename "$lib" .a)
        mkdir -p "${tmpdir}/${libname}"
        (cd "${tmpdir}/${libname}" && ar x "$lib")
    done
    cp "${build_dir}/graphviz_api.o" "${tmpdir}/"

    mkdir -p "${build_dir}/out"
    # Lib name matches all other platforms (linux/macos/android produce
    # libgraphviz_api.{so,dylib}); Rust build.rs links via
    # `cargo:rustc-link-lib=static=graphviz_api` so the .a MUST be named
    # libgraphviz_api.a for cargo to find it.
    ar rcs "${build_dir}/out/libgraphviz_api.a" "${tmpdir}"/*.o "${tmpdir}"/**/*.o 2>/dev/null || \
        find "${tmpdir}" -name "*.o" -exec ar rcs "${build_dir}/out/libgraphviz_api.a" {} +

    rm -rf "${tmpdir}"

    # Install per-slice artifacts under INSTALL_DIR so CI packaging and
    # Rust build.rs's try_repo_output can find them at a stable path:
    #   output/ios/<slice>/lib/libgraphviz_api.a
    #   output/ios/<slice>/include/graphviz_api.h
    # where <slice> is "${sdk}-${arch}" i.e. iphoneos-arm64,
    # iphonesimulator-arm64, iphonesimulator-x86_64.
    local slice_install="${INSTALL_DIR}/${sdk}-${arch}"
    mkdir -p "${slice_install}/lib" "${slice_install}/include"
    cp "${build_dir}/out/libgraphviz_api.a" "${slice_install}/lib/"
    cp "${WRAPPER_SRC}/graphviz_api.h" "${slice_install}/include/"
}

# Build for device (arm64) and both simulator slices (arm64 + x86_64).
# Three slices are required:
#   - iphoneos-arm64:        physical iPhone/iPad devices
#   - iphonesimulator-arm64: Apple-silicon Mac simulator
#   - iphonesimulator-x86_64 Intel Mac simulator (still needed for engineers
#                            on Intel hardware and some CI runners)
build_ios_arch "arm64" "iphoneos"
build_ios_arch "arm64" "iphonesimulator"
build_ios_arch "x86_64" "iphonesimulator"

# Public headers only (not the 200+ internal graphviz headers)
HEADER_DIR="${BUILD_DIR}/public-headers"
rm -rf "${HEADER_DIR}"
mkdir -p "${HEADER_DIR}"
cp "${WRAPPER_SRC}/graphviz_api.h" "${HEADER_DIR}/"

# Combine the two simulator slices into a single fat archive so that
# xcodebuild -create-xcframework sees exactly one library entry per
# platform (device vs simulator).  Passing two separate iphonesimulator
# .a files to -create-xcframework is NOT supported and will error with
# "error: library found in multiple slice locations for the same platform".
log_info "Lipo-ing simulator slices (arm64 + x86_64)..."
SIMULATOR_FAT="${BUILD_DIR}/iphonesimulator-fat/out"
mkdir -p "${SIMULATOR_FAT}"
lipo -create \
    "${BUILD_DIR}/iphonesimulator-arm64/out/libgraphviz_api.a" \
    "${BUILD_DIR}/iphonesimulator-x86_64/out/libgraphviz_api.a" \
    -output "${SIMULATOR_FAT}/libgraphviz_api.a"

# Create XCFramework
log_info "Creating XCFramework..."
rm -rf "${INSTALL_DIR}/Graphviz.xcframework"
mkdir -p "${INSTALL_DIR}"

xcodebuild -create-xcframework \
    -library "${BUILD_DIR}/iphoneos-arm64/out/libgraphviz_api.a" -headers "${HEADER_DIR}" \
    -library "${SIMULATOR_FAT}/libgraphviz_api.a" -headers "${HEADER_DIR}" \
    -output "${INSTALL_DIR}/Graphviz.xcframework"

log_info "Verifying outputs..."
verify_output "${INSTALL_DIR}/Graphviz.xcframework/Info.plist" "XCFramework"

log_info "iOS build complete: ${INSTALL_DIR}"
