cmake_minimum_required(VERSION 3.22.1)
project(graphviz_jni)

# Pre-built unified libgraphviz_api.so for this ABI — staged by
# scripts/prepare-native.js under jniLibs/<abi>/. We import it as a SHARED
# library so the host's APK bundles a single .so per ABI (single-sourced —
# the same file Gradle packages via sourceSets.main.jniLibs).
set(GRAPHVIZ_NATIVE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/jniLibs/${ANDROID_ABI}")
set(GRAPHVIZ_API_LIB "${GRAPHVIZ_NATIVE_DIR}/libgraphviz_api.so")

if(NOT EXISTS "${GRAPHVIZ_API_LIB}")
  message(FATAL_ERROR
    "Missing prebuilt Graphviz core library:\n"
    "  ${GRAPHVIZ_API_LIB}\n"
    "Run `./scripts/build-android.sh` at the crate root and then "
    "`node scripts/prepare-native.js` in this package."
  )
endif()

# The C ABI header is canonical at the crate root (capi/graphviz_api.h); inline
# it via an include path so we never duplicate the source of truth. From this
# CMakeLists (android/src/main) the crate root is five levels up.
set(GRAPHVIZ_API_INCLUDE "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../capi")

add_library(graphviz_core SHARED IMPORTED)
set_target_properties(graphviz_core PROPERTIES
  IMPORTED_NO_SONAME TRUE
  IMPORTED_LOCATION "${GRAPHVIZ_API_LIB}"
)

add_library(graphviz_jni SHARED
    jni/graphviz_jni.c
)

target_include_directories(graphviz_jni PRIVATE "${GRAPHVIZ_API_INCLUDE}")

# Link Android log library for JNI
find_library(log-lib log)
target_link_libraries(graphviz_jni graphviz_core ${log-lib})
