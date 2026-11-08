#!/bin/sh
# Format and lint gate for the Rust workspace: cargo fmt --check, then
# Clippy with every warning an error, on all targets (tests, examples).
#
#     tools/gates/rust_gate.sh            # native, plus --features slang when the library is built
#     tools/gates/rust_gate.sh --native   # native only
#     tools/gates/rust_gate.sh --slang    # also the Slang bridge; fails if the library is missing
#
# The slang leg needs libuvmcheck_model (cmake --preset model && cmake
# --build build-model --target uvmcheck_model), found the way
# crates/sv-slang/build.rs finds it: $UVMCHECK_BUILD_DIR, else build-model/.
# Formatting follows rustfmt.toml; Clippy reads the MSRV from Cargo.toml.
# Fix with `cargo fmt --all` and `cargo clippy --fix`.
set -eu
cd "$(dirname "$0")/../.."

slang=auto
case "${1-}" in
    --native) slang=no ;;
    --slang) slang=yes ;;
    "") ;;
    *) echo "usage: $0 [--native | --slang]" >&2; exit 2 ;;
esac

lib_dir=${UVMCHECK_BUILD_DIR:-build-model}
have_lib=no
for f in libuvmcheck_model.so libuvmcheck_model.dylib uvmcheck_model.dll; do
    [ -e "$lib_dir/$f" ] && have_lib=yes
done
if [ "$slang" = yes ] && [ "$have_lib" = no ]; then
    echo "rust_gate: --slang needs libuvmcheck_model in $lib_dir" >&2
    exit 2
fi
[ "$slang" = auto ] && slang=$have_lib

step() { echo "== $*"; "$@"; }

step cargo fmt --all --check
step cargo clippy --all-targets --locked -- -D warnings
if [ "$slang" = yes ]; then
    step cargo clippy --workspace --all-targets --locked --features slang -- -D warnings
else
    echo "== skipped the slang leg (no libuvmcheck_model in $lib_dir)"
fi
echo "rust_gate: clean"
