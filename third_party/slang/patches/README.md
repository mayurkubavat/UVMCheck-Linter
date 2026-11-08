# Local Slang patches

Empty on purpose: every Slang workaround so far is an override in
`cpp/src/compat/`. Put a patch here only when that is impossible, named
`NNNN-<upstream issue>-<short-name>.patch`, and follow
[../README.md](../README.md#adding-a-patch). `*.patch` files here are applied
in name order after CMake fetches the pinned commit.
