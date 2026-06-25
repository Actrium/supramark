cmake_minimum_required(VERSION 3.22.1)
project(graphviz_jni)

# Link against the prebuilt core library downloaded during install.
set(GRAPHVIZ_PREBUILT "${CMAKE_CURRENT_SOURCE_DIR}/../../libs/${ANDROID_ABI}")
set(GRAPHVIZ_API_INCLUDE "${GRAPHVIZ_PREBUILT}/include")
set(GRAPHVIZ_API_LIB "${GRAPHVIZ_PREBUILT}/lib/libgraphviz_api.so")

if(NOT EXISTS "${GRAPHVIZ_API_LIB}")
  message(FATAL_ERROR "Missing prebuilt Graphviz core library: ${GRAPHVIZ_API_LIB}")
endif()

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
