# Applies third_party/slang/patches/*.patch to the fetched slang checkout (the
# working directory). Run by FetchContent's patch step; PATCHES is a
# |-separated list. A patch that is already applied is skipped, so a re-run of
# the step is harmless; one that neither applies nor is applied stops the build.
string(REPLACE "|" ";" PATCHES "${PATCHES}")
foreach(p IN LISTS PATCHES)
    get_filename_component(name ${p} NAME)
    execute_process(COMMAND git apply --reverse --check ${p}
                    RESULT_VARIABLE applied OUTPUT_QUIET ERROR_QUIET)
    if(applied EQUAL 0)
        message(STATUS "slang patch ${name}: already applied")
        continue()
    endif()
    execute_process(COMMAND git apply --whitespace=nowarn ${p} RESULT_VARIABLE rc)
    if(NOT rc EQUAL 0)
        message(FATAL_ERROR "slang patch ${name} does not apply to this slang commit. "
                            "Rebase it, or delete it if upstream fixed the bug (third_party/slang/README.md).")
    endif()
    message(STATUS "slang patch ${name}: applied")
endforeach()
