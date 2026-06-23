cmake_minimum_required(VERSION 3.22.1)
project(supramark_markdown_jni)

# Pre-built libsupramark_markdown_native.so for this ABI — staged by
# scripts/prepare-native.js under jniLibs/<abi>/. We import it as a
# SHARED library so the host's APK bundles a single .so per ABI.
set(MARKDOWN_NATIVE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/jniLibs/${ANDROID_ABI}")
set(MARKDOWN_NATIVE_LIB "${MARKDOWN_NATIVE_DIR}/libsupramark_markdown_native.so")

if(NOT EXISTS "${MARKDOWN_NATIVE_LIB}")
    message(FATAL_ERROR
        "Missing prebuilt supramark-markdown-native library:\n"
        "  ${MARKDOWN_NATIVE_LIB}\n"
        "Run `cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build "
        "--release -p supramark-markdown-native` and then "
        "`node scripts/prepare-native.js` in this package."
    )
endif()

# Header path: prepare-native.js 把 C ABI header 拷贝到包内的
# jni/include/，让包自包含（file: 协议安装后包被拷贝到消费者 node_modules，
# 原先指向 monorepo 内 native crate 的相对路径会断裂）。
set(SUPRAMARK_MARKDOWN_INCLUDE
    "${CMAKE_CURRENT_SOURCE_DIR}/jni/include")

add_library(supramark_markdown_native SHARED IMPORTED)
set_target_properties(supramark_markdown_native PROPERTIES
    IMPORTED_NO_SONAME TRUE
    IMPORTED_LOCATION "${MARKDOWN_NATIVE_LIB}"
)

add_library(supramark_markdown_jni SHARED
    jni/supramark_markdown_jni.c
)

target_include_directories(supramark_markdown_jni PRIVATE
    "${SUPRAMARK_MARKDOWN_INCLUDE}"
)

find_library(log-lib log)
target_link_libraries(supramark_markdown_jni
    supramark_markdown_native
    ${log-lib}
)
