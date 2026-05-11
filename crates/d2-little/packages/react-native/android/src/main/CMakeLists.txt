cmake_minimum_required(VERSION 3.22.1)
project(supramark_d2_jni)

# Pre-built libsupramark_d2_native.so for this ABI — staged by
# scripts/prepare-native.js under jniLibs/<abi>/. We import it as a
# SHARED library so the host's APK bundles a single .so per ABI.
set(D2_NATIVE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/jniLibs/${ANDROID_ABI}")
set(D2_NATIVE_LIB "${D2_NATIVE_DIR}/libsupramark_d2_native.so")

if(NOT EXISTS "${D2_NATIVE_LIB}")
    message(FATAL_ERROR
        "Missing prebuilt supramark-d2-native library:\n"
        "  ${D2_NATIVE_LIB}\n"
        "Run `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build "
        "--release -p supramark-d2-native` and then "
        "`node scripts/prepare-native.js` in this package."
    )
endif()

# Header path: the C ABI header is canonical at the native crate
# (sibling-of-sibling-of-sibling directory). Inlining it via include
# path so we don't duplicate the source of truth.
set(SUPRAMARK_D2_INCLUDE
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../native/include")

add_library(supramark_d2_native SHARED IMPORTED)
set_target_properties(supramark_d2_native PROPERTIES
    IMPORTED_NO_SONAME TRUE
    IMPORTED_LOCATION "${D2_NATIVE_LIB}"
)

add_library(supramark_d2_jni SHARED
    jni/supramark_d2_jni.c
)

target_include_directories(supramark_d2_jni PRIVATE
    "${SUPRAMARK_D2_INCLUDE}"
)

find_library(log-lib log)
target_link_libraries(supramark_d2_jni
    supramark_d2_native
    ${log-lib}
)
