cmake_minimum_required(VERSION 3.22.1)
project(supramark_mermaid_jni)

# Pre-built libsupramark_mermaid_native.so for this ABI — staged by
# scripts/prepare-native.js under jniLibs/<abi>/. We import it as a
# SHARED library so the host's APK bundles a single .so per ABI.
set(Mermaid_NATIVE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/jniLibs/${ANDROID_ABI}")
set(Mermaid_NATIVE_LIB "${Mermaid_NATIVE_DIR}/libsupramark_mermaid_native.so")

if(NOT EXISTS "${Mermaid_NATIVE_LIB}")
    message(FATAL_ERROR
        "Missing prebuilt supramark-mermaid-native library:\n"
        "  ${Mermaid_NATIVE_LIB}\n"
        "Run `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build "
        "--release -p supramark-mermaid-native` and then "
        "`node scripts/prepare-native.js` in this package."
    )
endif()

# Header path: prepare-native.js stages the C ABI header into the package's
# jni/include/ so the package is self-contained. A `file:` install copies the
# package into the consumer's node_modules, which would break a relative path
# pointing back at the monorepo's native crate.
set(SUPRAMARK_MERMAID_INCLUDE
    "${CMAKE_CURRENT_SOURCE_DIR}/jni/include")

add_library(supramark_mermaid_native SHARED IMPORTED)
set_target_properties(supramark_mermaid_native PROPERTIES
    IMPORTED_NO_SONAME TRUE
    IMPORTED_LOCATION "${Mermaid_NATIVE_LIB}"
)

add_library(supramark_mermaid_jni SHARED
    jni/supramark_mermaid_jni.c
)

target_include_directories(supramark_mermaid_jni PRIVATE
    "${SUPRAMARK_MERMAID_INCLUDE}"
)

find_library(log-lib log)
target_link_libraries(supramark_mermaid_jni
    supramark_mermaid_native
    ${log-lib}
)
