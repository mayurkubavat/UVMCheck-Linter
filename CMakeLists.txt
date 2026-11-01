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

message(STATUS "Fetching Slang from GitHub")
include(FetchContent)
FetchContent_Declare(
    slang
    GIT_REPOSITORY https://github.com/MikePopoloski/slang.git
    # Pinned: rule findings and the test baselines depend on slang's elaboration and
    # diagnostics, which change between releases. Bump deliberately and refresh baselines.
    GIT_TAG v12.1
    GIT_SHALLOW ON)
FetchContent_MakeAvailable(slang)

# Core library sources
file(GLOB_RECURSE UVMCHECK_SOURCES
    src/core/*.cpp
    src/engine/*.cpp
    src/uvm/*.cpp
    src/rules/*.cpp
    src/waivers/*.cpp
    src/reporting/*.cpp
    src/style/*.cpp
    src/plugins/*.cpp
    src/lsp/*.cpp
)

find_package(Python3 COMPONENTS Interpreter REQUIRED)
# --explain data: each rule's tested bad/good example and its options, generated from
# tests/rules/ and the rule sources so the examples shown are the ones the tests run.
file(GLOB_RECURSE UVMCHECK_RULE_EXAMPLES CONFIGURE_DEPENDS
    ${CMAKE_CURRENT_SOURCE_DIR}/tests/rules/*.sv)
file(GLOB_RECURSE UVMCHECK_RULE_SOURCES CONFIGURE_DEPENDS
    ${CMAKE_CURRENT_SOURCE_DIR}/src/rules/*.cpp ${CMAKE_CURRENT_SOURCE_DIR}/src/style/*.cpp)
set(UVMCHECK_RULE_EXTRAS ${CMAKE_CURRENT_BINARY_DIR}/generated/RuleExtras.cpp)
add_custom_command(
    OUTPUT ${UVMCHECK_RULE_EXTRAS}
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/scripts/gen_rule_extras.py
            --repo ${CMAKE_CURRENT_SOURCE_DIR} --out ${UVMCHECK_RULE_EXTRAS}
    DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/scripts/gen_rule_extras.py ${UVMCHECK_RULE_EXAMPLES} ${UVMCHECK_RULE_SOURCES}
    COMMENT "Generating rule examples for --explain"
    VERBATIM)

add_library(libuvmcheck STATIC ${UVMCHECK_SOURCES} ${UVMCHECK_RULE_EXTRAS})
target_include_directories(libuvmcheck PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_link_libraries(libuvmcheck PUBLIC slang::slang)
# Fallback location of the Python SDK (config loader, plugin runner) for in-tree builds
target_compile_definitions(libuvmcheck PRIVATE
    UVMCHECK_PYTHON_SDK_DIR="${CMAKE_CURRENT_SOURCE_DIR}/python")

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
# Rules register themselves from static initializers in their own files, so the whole
# library is linked in; otherwise the linker drops rule objects nothing references.
target_link_libraries(uvmcheck PRIVATE "$<LINK_LIBRARY:WHOLE_ARCHIVE,libuvmcheck>" slang::slang)
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
    NAME all_rules_stub_suite
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
            --mode all-rules --baseline ${CMAKE_CURRENT_SOURCE_DIR}/tests/baselines/baseline_stub.json
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

# Runs against the real Accellera library when a uvm-core checkout is given,
# e.g. cmake -DUVMCHECK_UVM_HOME=/path/to/uvm-core
set(UVMCHECK_UVM_HOME "" CACHE PATH "uvm-core checkout used by the real-UVM rule tests")
if(UVMCHECK_UVM_HOME)
    add_test(
        NAME all_rules_uvm_core_suite
        COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
                --mode all-rules --uvm-home ${UVMCHECK_UVM_HOME}
                --baseline ${CMAKE_CURRENT_SOURCE_DIR}/tests/baselines/baseline_uvm_core.json
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    )
endif()
add_test(
    NAME project_config_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_config.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME methodology_guide_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_guidelines.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME rule_catalog_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_catalog.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME waiver_hygiene_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_waivers.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME changed_since_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_changed.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME fix_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_fix.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME result_cache_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_cache.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME report_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_report.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME precision_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_precision.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME testbench_map_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_map.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME explain_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_explain.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME init_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_init.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME findings_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_findings.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME baseline_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_baseline.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME engine_gate_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_engine_gate.py
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME python_rules_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_python_rules.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME history_rules_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_history.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME engine_equivalence
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/engine_equivalence.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)
add_test(
    NAME lsp_integration_test
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/test_lsp.py $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

