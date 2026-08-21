cmake_minimum_required(VERSION 3.22)
project(omni_builder LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT DEFINED OMNI_RUST_ARTIFACT_DIR)
    message(FATAL_ERROR
        "OMNI_RUST_ARTIFACT_DIR was not supplied. Gradle passes it; invoking "
        "this CMake file directly is not a supported entry point.")
endif()

if(NOT DEFINED OMNI_RUST_PROFILE)
    set(OMNI_RUST_PROFILE "release")
endif()

if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(OMNI_RUST_TARGET "aarch64-linux-android")
elseif(ANDROID_ABI STREQUAL "armeabi-v7a")
    set(OMNI_RUST_TARGET "armv7-linux-androideabi")
elseif(ANDROID_ABI STREQUAL "x86_64")
    set(OMNI_RUST_TARGET "x86_64-linux-android")
elseif(ANDROID_ABI STREQUAL "x86")
    set(OMNI_RUST_TARGET "i686-linux-android")
else()
    message(FATAL_ERROR
        "Unsupported ANDROID_ABI '${ANDROID_ABI}'. Supported: arm64-v8a, "
        "armeabi-v7a, x86_64, x86.")
endif()

set(OMNI_CORE_ARCHIVE
    "${OMNI_RUST_ARTIFACT_DIR}/${OMNI_RUST_TARGET}/${OMNI_RUST_PROFILE}/libomni_core.a")

if(NOT EXISTS "${OMNI_CORE_ARCHIVE}")
    message(FATAL_ERROR
        "Omni Core archive not found for ABI '${ANDROID_ABI}'.\n"
        "  Expected: ${OMNI_CORE_ARCHIVE}\n"
        "  Build it first, for example:\n"
        "    cargo build --release --target ${OMNI_RUST_TARGET}\n"
        "This build refuses to produce a library without the Core linked in.")
endif()

message(STATUS "Omni Core archive for ${ANDROID_ABI}: ${OMNI_CORE_ARCHIVE}")

add_library(omni_builder SHARED Builder.cpp)

target_include_directories(omni_builder PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")

target_compile_options(omni_builder PRIVATE
    -Wall
    -Wextra
    -Werror
    -fvisibility=hidden
    -fstack-protector-strong)

target_link_libraries(omni_builder PRIVATE
    "${OMNI_CORE_ARCHIVE}"
    log
    dl
    m)

target_link_options(omni_builder PRIVATE
    "-Wl,--exclude-libs,ALL"
    "-Wl,-z,relro"
    "-Wl,-z,now"
    "-Wl,--build-id=sha1")
