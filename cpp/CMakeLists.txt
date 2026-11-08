# The Slang-based uvmcheck engine: libuvmcheck, the uvmcheck CLI, the optional
# libuvmcheck_model shared library (C ABI) and their tests. Included from the
# top-level CMakeLists.txt, which fetches Slang and sets the compile options.
# Sources and headers are relative to this folder; shared inputs (testdata/,
# data/, tools/, python/) are relative to the repository root.

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
    src/compat/*.cpp
    src/model/*.cpp
)

find_package(Python3 COMPONENTS Interpreter REQUIRED)
# --explain data: each rule's tested bad/good example and its options, generated from
# testdata/rules/ and the rule sources so the examples shown are the ones the tests run.
file(GLOB_RECURSE UVMCHECK_RULE_EXAMPLES CONFIGURE_DEPENDS
    ${PROJECT_SOURCE_DIR}/testdata/rules/*.sv)
file(GLOB_RECURSE UVMCHECK_RULE_SOURCES CONFIGURE_DEPENDS
    ${CMAKE_CURRENT_SOURCE_DIR}/src/rules/*.cpp ${CMAKE_CURRENT_SOURCE_DIR}/src/style/*.cpp)
set(UVMCHECK_RULE_EXTRAS ${CMAKE_CURRENT_BINARY_DIR}/generated/RuleExtras.cpp)
add_custom_command(
    OUTPUT ${UVMCHECK_RULE_EXTRAS}
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tools/codegen/gen_rule_extras.py
            --repo ${PROJECT_SOURCE_DIR} --out ${UVMCHECK_RULE_EXTRAS}
    DEPENDS ${PROJECT_SOURCE_DIR}/tools/codegen/gen_rule_extras.py ${UVMCHECK_RULE_EXAMPLES} ${UVMCHECK_RULE_SOURCES}
    COMMENT "Generating rule examples for --explain"
    VERBATIM)

# UVM API packs (data/uvm-api/<version>.json, made by tools/codegen/gen_uvm_api_pack.py from a
# uvm-core checkout), compiled into the binary for uvm::ApiPack.
file(GLOB UVMCHECK_UVM_API_PACKS CONFIGURE_DEPENDS ${PROJECT_SOURCE_DIR}/data/uvm-api/*.json)
set(UVMCHECK_UVM_API_PACKS_CPP ${CMAKE_CURRENT_BINARY_DIR}/generated/UvmApiPacks.cpp)
add_custom_command(
    OUTPUT ${UVMCHECK_UVM_API_PACKS_CPP}
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tools/codegen/embed_uvm_api_packs.py
            --packs ${PROJECT_SOURCE_DIR}/data/uvm-api --out ${UVMCHECK_UVM_API_PACKS_CPP}
    DEPENDS ${PROJECT_SOURCE_DIR}/tools/codegen/embed_uvm_api_packs.py ${UVMCHECK_UVM_API_PACKS}
    COMMENT "Embedding UVM API packs"
    VERBATIM)

add_library(libuvmcheck STATIC ${UVMCHECK_SOURCES} ${UVMCHECK_RULE_EXTRAS} ${UVMCHECK_UVM_API_PACKS_CPP})
target_include_directories(libuvmcheck PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)
target_link_libraries(libuvmcheck PUBLIC slang::slang)
# Fallback location of the Python SDK (config loader, plugin runner) for in-tree builds
target_compile_definitions(libuvmcheck PRIVATE
    UVMCHECK_PYTHON_SDK_DIR="${PROJECT_SOURCE_DIR}/python")

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

# libuvmcheck_model: the slang-free design model (uvmcheck::model) behind a C
# ABI, for Rust (crates/), Python and other DV tools. Only the C API is exported.
if(UVMCHECK_BUILD_MODEL_LIB)
    add_library(uvmcheck_model SHARED src/capi/CApi.cpp)
    target_link_libraries(uvmcheck_model PRIVATE libuvmcheck slang::slang)
    set_target_properties(uvmcheck_model PROPERTIES
        CXX_VISIBILITY_PRESET hidden
        VISIBILITY_INLINES_HIDDEN ON)
    if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
        # Keep slang's and the linter's symbols private to the library.
        target_link_options(uvmcheck_model PRIVATE "LINKER:--exclude-libs,ALL")
    endif()
endif()

# CTest Integration (enable_testing() is in the top-level CMakeLists.txt)
add_test(
    NAME rule_regression_suite
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME all_rules_stub_suite
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
            --mode all-rules --baseline ${PROJECT_SOURCE_DIR}/cpp/tests/baselines/baseline_stub.json
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)

# Runs against the real Accellera library when a uvm-core checkout is given,
# e.g. cmake -DUVMCHECK_UVM_HOME=/path/to/uvm-core
set(UVMCHECK_UVM_HOME "" CACHE PATH "uvm-core checkout used by the real-UVM rule tests")
if(UVMCHECK_UVM_HOME)
    add_test(
        NAME all_rules_uvm_core_suite
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/regression.py --binary $<TARGET_FILE:uvmcheck>
                --mode all-rules --uvm-home ${UVMCHECK_UVM_HOME}
                --baseline ${PROJECT_SOURCE_DIR}/cpp/tests/baselines/baseline_uvm_core.json
        WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
    )
endif()
add_test(
    NAME project_config_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_config.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME slang_overlay_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_compat.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME lrm_conformance_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_conformance.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME ast_coverage_floor_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/tools/quality/ast_coverage.py
            --slang-include ${slang_SOURCE_DIR}/include
            --floor ${PROJECT_SOURCE_DIR}/testdata/conformance/ast_coverage_floor.json
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME config_sweep_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_config_sweep.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME methodology_guide_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_guidelines.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME rule_catalog_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_catalog.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME waiver_hygiene_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_waivers.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME changed_since_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_changed.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME fix_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_fix.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME fix_invariants_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_fix_invariants.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME result_cache_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_cache.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME report_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_report.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME precision_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_precision.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME testbench_map_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_map.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME explain_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_explain.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME init_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_init.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME findings_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_findings.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME baseline_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_baseline.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME engine_gate_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_engine_gate.py
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME python_rules_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/python/tests/test_python_rules.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME history_rules_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_history.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME engine_equivalence
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/engine_equivalence.py --binary $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)
add_test(
    NAME lsp_integration_test
    COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/cpp/tests/test_lsp.py $<TARGET_FILE:uvmcheck>
    WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
)


if(UVMCHECK_BUILD_MODEL_LIB)
    # The model's C ABI, exercised from a C program.
    enable_language(C)
    add_executable(model_capi_smoke tests/capi_smoke.c)
    target_include_directories(model_capi_smoke PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
    target_link_libraries(model_capi_smoke PRIVATE uvmcheck_model)
    add_test(
        NAME model_capi_test
        COMMAND model_capi_smoke ${PROJECT_SOURCE_DIR}/testdata/common
                ${PROJECT_SOURCE_DIR}/testdata/rules/oop/SV-OOP-007/bad.sv
                ${PROJECT_SOURCE_DIR}/testdata/model/facts.sv
        WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
    )
    # The Python binding (python/uvmcheck/model.py) over the same ABI.
    add_test(
        NAME model_python_test
        COMMAND ${Python3_EXECUTABLE} ${PROJECT_SOURCE_DIR}/python/tests/test_model_python.py
                --lib $<TARGET_FILE:uvmcheck_model>
        WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
    )
    # The Rust crates (crates/) against this build's library, when cargo is around.
    find_program(CARGO_PROGRAM cargo)
    if(CARGO_PROGRAM)
        add_test(
            NAME rust_model_test
            COMMAND ${CMAKE_COMMAND} -E env UVMCHECK_BUILD_DIR=${CMAKE_BINARY_DIR}
                    ${CARGO_PROGRAM} test --quiet --manifest-path ${PROJECT_SOURCE_DIR}/Cargo.toml
            WORKING_DIRECTORY ${PROJECT_SOURCE_DIR}
        )
    endif()
endif()
