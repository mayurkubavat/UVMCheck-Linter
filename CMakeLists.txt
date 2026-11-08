cmake_minimum_required(VERSION 3.28...3.31)
project(uvmcheck VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# The model facade is also built as a shared library (libuvmcheck_model) that
# Rust and other tools load through its C ABI. Everything it links, slang
# included, must then be position independent.
# mimalloc is off in that build: slang links it statically, and a shared
# library loaded into a host (Rust, Python) must not mix two allocators
# (slang's own Python bindings turn it off for the same reason). The linter
# CLI keeps both, so the default build leaves this OFF; use the "model" preset.
option(UVMCHECK_BUILD_MODEL_LIB "Build the shared model library with a C ABI" OFF)
if(UVMCHECK_BUILD_MODEL_LIB)
    set(CMAKE_POSITION_INDEPENDENT_CODE ON)
    set(SLANG_USE_MIMALLOC OFF CACHE BOOL "" FORCE)
endif()

# Enable compiler cache (ccache) if available to accelerate rebuilds
find_program(CCACHE_PROGRAM ccache)
if(CCACHE_PROGRAM)
    message(STATUS "Using ccache: ${CCACHE_PROGRAM}")
    set(CMAKE_CXX_COMPILER_LAUNCHER "${CCACHE_PROGRAM}")
endif()

# Throttle concurrent compile jobs when using Ninja to prevent OOM
# On 16GB RAM machines, max 4 heavy C++20 Clang processes prevents memory exhaustion & swap thrashing
set_property(GLOBAL PROPERTY JOB_POOLS compile_job_pool=4 link_job_pool=1)
set(CMAKE_JOB_POOL_COMPILE compile_job_pool)
set(CMAKE_JOB_POOL_LINK link_job_pool)

# Compile options
if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|AppleClang|GNU")
    add_compile_options(-Wall -Wextra -Wpedantic -Wno-unused-parameter $<$<COMPILE_LANGUAGE:CXX>:-fvisibility-inlines-hidden>)
endif()

# Disable unneeded slang targets to keep build fast
set(SLANG_INCLUDE_TOOLS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_DOCS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_INSTALL OFF CACHE BOOL "" FORCE)

message(STATUS "Fetching Slang from GitHub")
include(FetchContent)
FetchContent_Declare(
    slang
    GIT_REPOSITORY https://github.com/MikePopoloski/slang.git
    # Pinned: rule findings and the test baselines depend on slang's elaboration and
    # diagnostics, which change between releases. Bump deliberately and refresh baselines.
    # A master commit after v12.1: it has the randomize/constraint scoping fixes and the
    # macro location API. Move back to a release tag once one includes them.
    GIT_TAG d02b7cda02d3183f35c6c4d4db2b2fc03f19f4ae # master, 2026-11-02
    # A commit hash cannot be fetched shallowly.
    GIT_SHALLOW OFF)
FetchContent_MakeAvailable(slang)

# Every binary and library lands at the top of the build tree (build/uvmcheck,
# build-model/libuvmcheck_model.so), wherever its CMakeLists.txt lives.
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR})
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR})
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR})

enable_testing()
find_package(Python3 COMPONENTS Interpreter REQUIRED)

# The C++ engine (uvmcheck, libuvmcheck_model) and its tests.
add_subdirectory(cpp)

# Reference dump of Slang's preprocessor, for checking crates/sv-preproc.
add_executable(uvmcheck_pp_dump tools/oracles/pp_dump.cpp)
target_link_libraries(uvmcheck_pp_dump PRIVATE slang::slang)

# Reference outline from Slang's parser, for checking crates/sv-syntax.
add_executable(uvmcheck_cst_outline tools/oracles/cst_outline.cpp)
target_link_libraries(uvmcheck_cst_outline PRIVATE slang::slang)
add_executable(uvmcheck_sym_dump tools/oracles/sym_dump.cpp)
target_link_libraries(uvmcheck_sym_dump PRIVATE slang::slang)
add_executable(uvmcheck_cst_exprs tools/oracles/cst_exprs.cpp)
target_link_libraries(uvmcheck_cst_exprs PRIVATE slang::slang)
# Reference expression types, for checking dv-lint's typer (tools/gates/expr_gate.py).
add_executable(uvmcheck_exprdump tools/oracles/exprdump.cpp)
target_link_libraries(uvmcheck_exprdump PRIVATE slang::slang)
