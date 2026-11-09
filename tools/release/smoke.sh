#!/bin/sh
# Checks a release archive the way a user meets it: unpacked somewhere else,
# with nothing of the repository beside it.
#
#     tools/release/smoke.sh uvmcheck-TARGET.tar.gz
#
# Lints a small testbench (text, JSON and SARIF), a project described by a
# Python config (so the SDK shipped in the archive is found), and starts
# every other tool.
set -eu
[ $# -eq 1 ] || { echo "usage: $0 ARCHIVE" >&2; exit 2; }
archive=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -C "$work" -xzf "$archive"
root=$(echo "$work"/uvmcheck-*)
bin=$root/bin
cd "$work"

cat > tb.sv <<'SV'
class smoke_item;
  rand int unsigned addr;
  function void roll();
    void'(this.randomize());
    if (addr == addr) $display("same");
  endfunction
endclass
SV

"$bin/uvmcheck" --list | grep -q '^SV-COR-008 ' || { echo "smoke: --list has no SV-COR-008" >&2; exit 1; }

status=0
"$bin/uvmcheck" --no-config --no-cache tb.sv --json out.json --sarif out.sarif > out.txt || status=$?
[ "$status" -eq 1 ] || { echo "smoke: expected exit 1 (an ERROR finding), got $status" >&2; cat out.txt >&2; exit 1; }
grep -q 'SV-COR-008' out.txt || { echo "smoke: SV-COR-008 not reported" >&2; cat out.txt >&2; exit 1; }
grep -q '"ruleId": "SV-COR-008"' out.json || { echo "smoke: JSON report lacks the finding" >&2; exit 1; }
grep -q '"version": "2.1.0"' out.sarif || { echo "smoke: SARIF report is not 2.1.0" >&2; exit 1; }

# A project config in Python: needs python3 and the SDK from the archive.
if command -v python3 >/dev/null; then
    mkdir -p proj/.uvmcheck
    cp tb.sv proj/tb.sv
    cat > proj/.uvmcheck/config.py <<'PY'
from uvmcheck.config import ProjectConfig, RuleConfig


class Project(ProjectConfig):
    sources = ["tb.sv"]

    class SelfComparison(RuleConfig):
        enabled = False
PY
    status=0
    (cd proj && "$bin/uvmcheck" --no-cache > ../proj.txt 2> ../proj.err) || status=$?
    if [ "$status" -ge 2 ] || grep -q 'SV-COR-008' proj.txt; then
        echo "smoke: the Python project config was not applied (exit $status)" >&2
        cat proj.txt proj.err >&2
        exit 1
    fi
fi

"$bin/dv-build" tb.sv > /dev/null
"$bin/dv-filelist" tb.sv > /dev/null
"$bin/dv-map" tb.sv > /dev/null
"$bin/dv-lsp" --version | grep -q '^dv-lsp ' || { echo "smoke: dv-lsp --version" >&2; exit 1; }
echo "smoke: $(basename "$archive") ok"
