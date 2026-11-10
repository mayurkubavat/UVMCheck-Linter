#!/usr/bin/env bash
# Everything CI runs on a push, run here first: the Rust workflow (rust.yml)
# and the C++ workflow (ci.yml). Run it before every push; a batch is ready
# when it ends with "prepush: all green".
#
#   tools/gates/prepush.sh              # both flows
#   tools/gates/prepush.sh --rust       # the Rust flow only
#   tools/gates/prepush.sh --cpp        # the C++ flow only
#   tools/gates/prepush.sh --quick      # both, without the two slow steps
#                                       # (all rules on real uvm-core, dist build)
#
# UVM_HOME must point at a uvm-core checkout of the tag CI uses (2020.3.1)
# for the uvm-core steps; without it they are reported as NOT RUN and the
# script fails, so a push never goes out half checked. --quick skips them
# on purpose and says so.
#
# A fixture edit changes the all-rules baselines. Regenerate both with:
#   python3 cpp/tests/regression.py --binary build/uvmcheck --mode all-rules \
#       --baseline cpp/tests/baselines/baseline_stub.json --update-baseline
#   python3 cpp/tests/regression.py --binary build/uvmcheck --mode all-rules \
#       --uvm-home "$UVM_HOME" --baseline cpp/tests/baselines/baseline_uvm_core.json --update-baseline
set -u
cd "$(dirname "$0")/../.."

rust=1 cpp=1 quick=0
for a in "$@"; do
    case "$a" in
        --rust) cpp=0 ;;
        --cpp) rust=0 ;;
        --quick) quick=1 ;;
        *) echo "usage: tools/gates/prepush.sh [--rust|--cpp] [--quick]" >&2; exit 2 ;;
    esac
done

jobs=${PREPUSH_JOBS:-4}
log=$(mktemp -d)
failed=() skipped=()

# step NAME COMMAND...: runs it, keeps the output, prints one line.
step() {
    local name=$1; shift
    local out="$log/$(echo "$name" | tr -c 'A-Za-z0-9' '_').log"
    local t0=$SECONDS
    if "$@" >"$out" 2>&1; then
        printf '  ok    %-52s %4ss\n' "$name" $((SECONDS - t0))
    else
        printf '  FAIL  %-52s %4ss   (%s)\n' "$name" $((SECONDS - t0)) "$out"
        tail -n 15 "$out" | sed 's/^/        /'
        failed+=("$name")
    fi
}
skip() {
    printf '  ----  %-52s %s\n' "$1" "$2"
    skipped+=("$1")
}
uvm_ok() { [ -n "${UVM_HOME:-}" ] && [ -f "$UVM_HOME/src/uvm_pkg.sv" ]; }

if [ "$rust" = 1 ]; then
    echo "Rust flow (rust.yml)"
    step "rust_gate: profiles, fmt, clippy" tools/gates/rust_gate.sh --native
    step "cargo test" cargo test --locked
    step "rule docs are current" sh -c 'cargo build --locked -q -p dv-lint --bin uvmcheck && python3 tools/codegen/gen_rule_docs.py --binary target/debug/uvmcheck --check'
    if [ "$quick" = 1 ]; then
        skip "release archive and smoke test" "skipped by --quick"
    else
        step "dist build" cargo build --locked --profile dist
        plat="$(uname -s | tr 'A-Z' 'a-z')-$(uname -m | sed 's/x86_64/x64/; s/aarch64/arm64/')"
        step "release archive and smoke test" sh -c \
            "rm -rf '$log/dist' && tools/release/package.sh target/dist '$plat' '$log/dist' && tools/release/smoke.sh '$log/dist/uvmcheck-$plat.tar.gz'"
    fi
    step "build uvmcheck (release)" cargo build --locked --release -p dv-lint --bin uvmcheck
    if uvm_ok; then
        step "ERROR budget (uvm-core)" python3 tools/quality/error_budget.py --binary target/release/uvmcheck
    else
        skip "ERROR budget (uvm-core)" "NOT RUN: set UVM_HOME to a uvm-core 2020.3.1 checkout"
        failed+=("ERROR budget (uvm-core): UVM_HOME not set")
    fi
fi

if [ "$cpp" = 1 ]; then
    echo "C++ flow (ci.yml)"
    [ -f build/build.ninja ] || step "configure" cmake --preset reference
    step "build" cmake --build build -j "$jobs"
    b=build/uvmcheck
    step "rules, isolated, UVM stub" python3 cpp/tests/regression.py --binary $b --mode isolated
    step "rules, all rules, UVM stub" python3 cpp/tests/regression.py --binary $b --mode all-rules \
        --baseline cpp/tests/baselines/baseline_stub.json
    if [ "$quick" = 1 ]; then
        skip "rules, all rules, real uvm-core" "skipped by --quick"
    elif uvm_ok; then
        step "rules, all rules, real uvm-core" python3 cpp/tests/regression.py --binary $b --mode all-rules \
            --uvm-home "$UVM_HOME" --baseline cpp/tests/baselines/baseline_uvm_core.json
    else
        skip "rules, all rules, real uvm-core" "NOT RUN: set UVM_HOME to a uvm-core 2020.3.1 checkout"
        failed+=("rules, all rules, real uvm-core: UVM_HOME not set")
    fi
    step "LSP" python3 cpp/tests/test_lsp.py
    for t in catalog config baseline findings init waivers changed fix fix_invariants report precision map explain \
        guidelines history; do
        step "test_$t" python3 "cpp/tests/test_$t.py" --binary $b
    done
    step "test_engine_gate" python3 cpp/tests/test_engine_gate.py
    step "python rules" python3 python/tests/test_python_rules.py --binary $b
    step "engine_equivalence" python3 cpp/tests/engine_equivalence.py --binary $b
fi

echo
if [ ${#failed[@]} -gt 0 ]; then
    echo "prepush: ${#failed[@]} FAILED, do not push"
    printf '  %s\n' "${failed[@]}"
    echo "logs: $log"
    exit 1
fi
if [ ${#skipped[@]} -gt 0 ]; then
    echo "prepush: green, but ${#skipped[@]} step(s) skipped by --quick: run without it before the push"
    exit 0
fi
echo "prepush: all green"
