cmake_minimum_required(VERSION 3.21)

project(native_webrtc_h264 LANGUAGES C CXX OBJCXX)

if(NOT DEFINED LIBDATACHANNEL_SOURCE_DIR)
    message(FATAL_ERROR "LIBDATACHANNEL_SOURCE_DIR is required")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(BUILD_SHARED_DEPS_LIBS OFF CACHE BOOL "" FORCE)
set(NO_TESTS ON CACHE BOOL "" FORCE)
set(NO_EXAMPLES ON CACHE BOOL "" FORCE)
set(USE_GNUTLS OFF CACHE BOOL "" FORCE)
set(USE_MBEDTLS OFF CACHE BOOL "" FORCE)
set(USE_NICE OFF CACHE BOOL "" FORCE)
set(PREFER_SYSTEM_LIB OFF CACHE BOOL "" FORCE)
set(RTC_UPDATE_VERSION_HEADER OFF CACHE BOOL "" FORCE)

add_subdirectory(
    "${LIBDATACHANNEL_SOURCE_DIR}"
    "${CMAKE_BINARY_DIR}/libdatachannel"
    EXCLUDE_FROM_ALL
)

add_executable(native-webrtc-h264 Sources/main.mm)
set_target_properties(native-webrtc-h264 PROPERTIES
    CXX_STANDARD 17
    CXX_STANDARD_REQUIRED YES
    OBJCXX_STANDARD 17
    OBJCXX_STANDARD_REQUIRED YES
)

target_link_libraries(native-webrtc-h264 PRIVATE
    LibDataChannel::LibDataChannelStatic
    "-framework AppKit"
    "-framework AVFoundation"
    "-framework CoreMedia"
    "-framework CoreVideo"
    "-framework QuartzCore"
    "-framework VideoToolbox"
)

target_compile_options(native-webrtc-h264 PRIVATE -Wall -Wextra -Wno-deprecated-declarations)

add_executable(android-webrtc-receiver Sources/android_receiver.mm)
set_target_properties(android-webrtc-receiver PROPERTIES
    CXX_STANDARD 17
    CXX_STANDARD_REQUIRED YES
    OBJCXX_STANDARD 17
    OBJCXX_STANDARD_REQUIRED YES
)
target_link_libraries(android-webrtc-receiver PRIVATE
    LibDataChannel::LibDataChannelStatic
    "-framework AppKit"
    "-framework AVFoundation"
    "-framework CoreMedia"
    "-framework CoreVideo"
    "-framework QuartzCore"
    "-framework VideoToolbox"
)
target_compile_options(android-webrtc-receiver PRIVATE -Wall -Wextra -Wno-deprecated-declarations)

add_executable(receiver-diagnostics-test Sources/receiver_diagnostics_test.cpp)
set_target_properties(receiver-diagnostics-test PROPERTIES CXX_STANDARD 17 CXX_STANDARD_REQUIRED YES)
target_compile_options(receiver-diagnostics-test PRIVATE -Wall -Wextra -Werror)
