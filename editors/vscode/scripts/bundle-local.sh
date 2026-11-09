#!/bin/sh
# Copies the locally built server into bin/<platform>-<arch>/ so `npm run
# package` makes a self-contained .vsix for this machine. Build it first:
#   cargo build --release -p dv-lsp
set -e
here=$(cd "$(dirname "$0")/.." && pwd)
repo=$(cd "$here/../.." && pwd)
plat=$(node -p 'process.platform + "-" + process.arch')
dest="$here/bin/$plat"
mkdir -p "$dest"
found=0
for src in "${DV_LSP_BIN:-$repo/target/release/dv-lsp}"; do
    if [ -x "$src" ]; then
        cp "$src" "$dest/"
        echo "bundled $(basename "$src") -> bin/$plat/"
        found=$((found + 1))
    else
        echo "not found (skipped): $src"
    fi
done
[ "$found" -gt 0 ] || { echo "nothing to bundle" >&2; exit 1; }
