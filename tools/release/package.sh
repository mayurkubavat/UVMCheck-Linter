#!/bin/sh
# Packages the native tools as the release ships them.
#
#     tools/release/package.sh BIN_DIR TARGET OUT_DIR
#
# BIN_DIR holds the built binaries (target/<triple>/dist), TARGET names the
# platform (linux-x64, darwin-arm64, ...). Writes
# OUT_DIR/uvmcheck-TARGET.tar.gz:
#
#     uvmcheck-TARGET/bin/      uvmcheck, dv-lsp, dv-build, dv-map, dv-filelist
#     uvmcheck-TARGET/python/   the Python SDK (config.py, Python rules);
#                               uvmcheck finds it beside bin/
#     uvmcheck-TARGET/LICENSE, NOTICE, README.md
set -eu
[ $# -eq 3 ] || { echo "usage: $0 BIN_DIR TARGET OUT_DIR" >&2; exit 2; }
bin=$1 target=$2 out=$3
repo=$(cd "$(dirname "$0")/../.." && pwd)
name=uvmcheck-$target
stage=$out/$name
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/python"
for tool in uvmcheck dv-lsp dv-build dv-map dv-filelist; do
    cp "$bin/$tool" "$stage/bin/$tool"
    chmod 755 "$stage/bin/$tool"
done
cp -R "$repo/python/uvmcheck" "$stage/python/uvmcheck"
find "$stage/python" -name __pycache__ -type d -prune -exec rm -rf {} +
cp "$repo/LICENSE" "$repo/NOTICE" "$stage/"
cp "$repo/tools/release/README.md" "$stage/README.md"
tar -C "$out" -czf "$out/$name.tar.gz" "$name"
rm -rf "$stage"
echo "$out/$name.tar.gz"
