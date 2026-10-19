cmake_minimum_required(VERSION 3.28...3.31)
project(uvmcheck VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

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
    add_compile_options(-Wall -Wextra -Wpedantic -Wno-unused-parameter -fvisibility-inlines-hidden)
endif()

# Disable unneeded slang targets to keep build fast
set(SLANG_INCLUDE_TOOLS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_DOCS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_INSTALL OFF CACHE BOOL "" FORCE)

if(EXISTS "${CMAKE_CURRENT_SOURCE_DIR}/references/code/slang/CMakeLists.txt")
    message(STATUS "Using local Slang clone in references/code/slang")
    add_subdirectory(references/code/slang EXCLUDE_FROM_ALL)
else()
    message(STATUS "Fetching Slang from GitHub")
    include(FetchContent)
    FetchContent_Declare(
        slang
        GIT_REPOSITORY https://github.com/MikePopoloski/slang.git
        GIT_SHALLOW ON)
    FetchContent_MakeAvailable(slang)
endif()

# Core library sources
file(GLOB_RECURSE UVMCHECK_SOURCES
    src/core/*.cpp
    src/uvm/*.cpp
    src/rules/*.cpp
    src/waivers/*.cpp
    src/reporting/*.cpp
    src/style/*.cpp
    src/plugins/*.cpp
    src/lsp/*.cpp
)

add_library(libuvmcheck STATIC ${UVMCHECK_SOURCES})
target_include_directories(libuvmcheck PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_link_libraries(libuvmcheck PUBLIC slang::slang)

# Precompile heavy C++20 and Slang AST headers to avoid re-parsing them 237 times
target_precompile_headers(libuvmcheck PRIVATE
    <string>
    <vector>
    <memory>
    <string_view>
    <unordered_map>
    <variant>
    <optional>
    <slang/ast/ASTVisitor.h>
    <slang/syntax/AllSyntax.h>
    <slang/ast/Compilation.h>
    <uvmcheck/rules/Rule.h>
    <uvmcheck/rules/RuleRegistry.h>
    <uvmcheck/core/Diagnostic.h>
)

# Optional Unity build for accelerating full rebuilds with bounded batch size
option(UVMCHECK_ENABLE_UNITY_BUILD "Enable Unity build for rules" OFF)
if(UVMCHECK_ENABLE_UNITY_BUILD)
    set_target_properties(libuvmcheck PROPERTIES
        UNITY_BUILD ON
        UNITY_BUILD_BATCH_SIZE 8
    )
endif()

# CLI executable
add_executable(uvmcheck src/main.cpp)
target_link_libraries(uvmcheck PRIVATE libuvmcheck slang::slang)
set_target_properties(uvmcheck PROPERTIES OUTPUT_NAME "uvmcheck")

# CTest Integration
enable_testing()
find_package(Python3 COMPONENTS Interpreter REQUIRED)
add_test(
    NAME rule_regression_suite
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME uvm_rules_suite
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/regression.py --binary $<TARGET_FILE:uvmcheck> --category uvm
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME lsp_integration_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_lsp.py
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

