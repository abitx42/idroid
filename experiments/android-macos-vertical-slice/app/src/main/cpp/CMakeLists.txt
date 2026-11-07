cmake_minimum_required(VERSION 3.22.1)
project(idroid_core_jni)

add_library(idroid_core_jni SHARED idroid_core_jni.cpp)
target_include_directories(idroid_core_jni PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../core-ffi/include"
)

if (NOT IDROID_CORE_FFI_LIB_DIR)
    message(FATAL_ERROR "IDROID_CORE_FFI_LIB_DIR is required for the Rust core JNI bridge")
endif()

add_library(idroid_core_ffi STATIC IMPORTED)
set_target_properties(idroid_core_ffi PROPERTIES
    IMPORTED_LOCATION "${IDROID_CORE_FFI_LIB_DIR}/libidroid_core_ffi.a"
)
find_library(log-lib log)
target_link_libraries(idroid_core_jni idroid_core_ffi ${log-lib})
