# Omni_Builder — native build for the JNI bridge
#
# NAME NOTE (ADR-0001 in Omni.rs)
# ------------------------------------------------------------------------------
# Directive section 46 spells this file "CMakelist.txt". CMake resolves only
# "CMakeLists.txt", so the file carries the name the tool requires. The location
# and the role are exactly as the directive specifies.
#
# What this file does
# ------------------------------------------------------------------------------
# Builds libomni_builder.so from Builder.cpp and links the Rust Core into it as a
# static archive. It never compiles Rust: cargo does that first, and this file
# fails loudly if the archive it expects is not there. A build that quietly
# produced a library without the Core would be exactly the kind of fake artifact
# directive section 1 forbids.

cmake_minimum_required(VERSION 3.22)
project(omni_builder LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# ------------------------------------------------------------------------------
# Inputs supplied by Gradle (Builder/build.gradle.kts).
# ------------------------------------------------------------------------------
if(NOT DEFINED OMNI_RUST_ARTIFACT_DIR)
    message(FATAL_ERROR
        "OMNI_RUST_ARTIFACT_DIR was not supplied. Gradle passes it; invoking "
        "this CMake file directly is not a supported entry point.")
endif()

if(NOT DEFINED OMNI_RUST_PROFILE)
    set(OMNI_RUST_PROFILE "release")
endif()

# ------------------------------------------------------------------------------
# Android ABI -> Rust target triple.
#
# The mapping is explicit rather than derived, so an unsupported ABI stops the
# build instead of silently linking the wrong architecture.
# ------------------------------------------------------------------------------
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

# ------------------------------------------------------------------------------
# The JNI bridge.
# ------------------------------------------------------------------------------
add_library(omni_builder SHARED Builder.cpp)

target_include_directories(omni_builder PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")

target_compile_options(omni_builder PRIVATE
    -Wall
    -Wextra
    -Werror
    -fvisibility=hidden
    -fstack-protector-strong)

# The Rust standard library needs these Android system libraries; `log` is also
# used directly by Builder.cpp.
target_link_libraries(omni_builder PRIVATE
    "${OMNI_CORE_ARCHIVE}"
    log
    dl
    m)

# Only the JNI entry points are exported. Nothing from the Core's C ABI leaks
# into the dynamic symbol table of the shared object.
target_link_options(omni_builder PRIVATE
    "-Wl,--exclude-libs,ALL"
    "-Wl,-z,relro"
    "-Wl,-z,now"
    "-Wl,--build-id=sha1")
