# Slang (https://github.com/MikePopoloski/slang): the upstream compiler front end
# uvmcheck is built on. It is someone else's code, so it never lives in this
# repository and is never edited in place: CMake fetches the pinned commit into
# the build tree (<build>/_deps/slang-src) and applies the patches in
# patches/, if any. How to bump the pin, add or retire a patch: README.md here.

# Pinned: rule findings and the test baselines depend on slang's elaboration and
# diagnostics, which change between releases. Bump deliberately and refresh baselines.
# A master commit after v12.1: it has the randomize/constraint scoping fixes and the
# macro location API. Move back to a release tag once one includes them.
set(UVMCHECK_SLANG_REPOSITORY https://github.com/MikePopoloski/slang.git)
set(UVMCHECK_SLANG_TAG d02b7cda02d3183f35c6c4d4db2b2fc03f19f4ae) # master, 2026-11-02

# Disable unneeded slang targets to keep build fast
set(SLANG_INCLUDE_TOOLS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_TESTS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_DOCS OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SLANG_INCLUDE_INSTALL OFF CACHE BOOL "" FORCE)

# Local patches: a last resort (see README.md). Applied in name order after the
# download; apply_patches.cmake skips ones that are already applied.
file(GLOB UVMCHECK_SLANG_PATCHES ${CMAKE_CURRENT_LIST_DIR}/patches/*.patch)
list(SORT UVMCHECK_SLANG_PATCHES)
set(UVMCHECK_SLANG_PATCH_ARGS)
if(UVMCHECK_SLANG_PATCHES)
    list(LENGTH UVMCHECK_SLANG_PATCHES n)
    message(STATUS "Slang: ${n} local patch(es) from third_party/slang/patches")
    string(REPLACE ";" "|" joined "${UVMCHECK_SLANG_PATCHES}")
    set(UVMCHECK_SLANG_PATCH_ARGS PATCH_COMMAND ${CMAKE_COMMAND}
        "-DPATCHES=${joined}" -P ${CMAKE_CURRENT_LIST_DIR}/apply_patches.cmake)
    if(FETCHCONTENT_SOURCE_DIR_SLANG)
        message(WARNING "FETCHCONTENT_SOURCE_DIR_SLANG is set: the patches in "
                        "third_party/slang/patches are NOT applied to ${FETCHCONTENT_SOURCE_DIR_SLANG}")
    endif()
endif()

message(STATUS "Fetching Slang ${UVMCHECK_SLANG_TAG}")
include(FetchContent)
FetchContent_Declare(
    slang
    GIT_REPOSITORY ${UVMCHECK_SLANG_REPOSITORY}
    GIT_TAG ${UVMCHECK_SLANG_TAG}
    # A commit hash cannot be fetched shallowly.
    GIT_SHALLOW OFF
    ${UVMCHECK_SLANG_PATCH_ARGS})
FetchContent_MakeAvailable(slang)
