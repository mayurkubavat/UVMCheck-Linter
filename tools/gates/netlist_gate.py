#!/usr/bin/env python3
"""RTL netlist lowering gate (`dv-build --netlist`) across `testdata/netlist/`.

Verifies:
1. Every valid RTL testcase lowers cleanly (`exit 0`, `errors 0`, non-empty `netlist <top>`),
   and emits byte-for-byte deterministic output across repeated invocations.
2. CLI flags (`-G name=value`, `--blast-memories <N>`, `--allow-four-state-ops`) work end-to-end.
3. Every fail-closed testcase (`overlap_drivers.sv`, `clock_typo.sv`, `reset_typo.sv`,
   `comb_loop.sv`, and default `isunknown.sv`) exits non-zero (`exit 1`) with the expected
   diagnostic message and never silently emits a broken netlist.

    cargo build --release -p dv-build
    python3 tools/gates/netlist_gate.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
NETLIST_DIR = os.path.join(REPO, "testdata", "netlist")


def find_dv_build() -> str:
    env_bin = os.environ.get("DV_BUILD")
    if env_bin and os.path.isfile(env_bin):
        return env_bin
    for profile in ("release", "debug"):
        cand = os.path.join(REPO, "target", profile, "dv-build")
        if os.path.isfile(cand):
            return cand
    subprocess.run(["cargo", "build", "-p", "dv-build"], cwd=REPO, check=True)
    return os.path.join(REPO, "target", "debug", "dv-build")


EXPECTED_FAIL = {
    "clock_typo.sv": "undeclared",
    "comb_loop.sv": "combinational loop",
    "isunknown.sv": "$isunknown",
    "overlap_drivers.sv": "overlapping drivers",
    "reset_typo.sv": "undeclared",
}


def run_dv(dv_build: str, extra_args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [dv_build, "--netlist", *extra_args],
        cwd=REPO,
        capture_output=True,
        text=True,
        errors="replace",
        timeout=60,
    )


def main() -> int:
    dv_build = find_dv_build()
    sv_files = sorted(f for f in os.listdir(NETLIST_DIR) if f.endswith(".sv"))
    if not sv_files:
        print("error: no .sv files found in testdata/netlist", file=sys.stderr)
        return 2

    passed = 0
    failures: list[str] = []

    for fname in sv_files:
        fpath = os.path.join(NETLIST_DIR, fname)
        if fname in EXPECTED_FAIL:
            needle = EXPECTED_FAIL[fname]
            r = run_dv(dv_build, [fpath])
            if r.returncode != 1:
                failures.append(f"{fname}: expected exit 1, got {r.returncode}\n{r.stdout}{r.stderr}")
                continue
            if "\nnetlist " in ("\n" + r.stdout):
                failures.append(f"{fname}: fail-closed testcase unexpectedly emitted a netlist")
                continue
            if needle not in r.stdout and needle not in r.stderr:
                failures.append(f"{fname}: missing expected diagnostic {needle!r} in:\n{r.stdout}{r.stderr}")
                continue
            passed += 1
        else:
            r1 = run_dv(dv_build, [fpath])
            r2 = run_dv(dv_build, [fpath])
            if r1.returncode != 0:
                failures.append(f"{fname}: expected exit 0, got {r1.returncode}\n{r1.stdout}{r1.stderr}")
                continue
            if not r1.stdout.startswith("netlist ") or "errors 0" not in r1.stdout:
                failures.append(f"{fname}: malformed netlist dump output:\n{r1.stdout}")
                continue
            if r1.stdout != r2.stdout:
                failures.append(f"{fname}: non-deterministic netlist dump across consecutive runs")
                continue
            passed += 1

    # Additional CLI flag gates:
    # 1. `-G W=6` parameter override on `counter_ok.sv`
    rc = run_dv(dv_build, ["-G", "W=6", os.path.join(NETLIST_DIR, "counter_ok.sv")])
    if rc.returncode != 0 or "output cnt : u6.4" not in rc.stdout:
        failures.append(f"counter_ok.sv (-G W=6): expected u6.4 output, got rc={rc.returncode}:\n{rc.stdout}")
    else:
        passed += 1

    # 2. `--blast-memories 16` on `fifo.sv`
    rb = run_dv(dv_build, ["--blast-memories", "16", os.path.join(NETLIST_DIR, "fifo.sv")])
    if rb.returncode != 0 or "\nmemory " in rb.stdout or "sync_fifo.mem[0]" not in rb.stdout:
        failures.append(f"fifo.sv (--blast-memories 16): expected blasted mem[0..3] states, got rc={rb.returncode}:\n{rb.stdout}")
    else:
        passed += 1

    # 3. `--allow-four-state-ops` on `isunknown.sv`
    ri = run_dv(dv_build, ["--allow-four-state-ops", os.path.join(NETLIST_DIR, "isunknown.sv")])
    if ri.returncode != 0 or "isunknown(" not in ri.stdout:
        failures.append(f"isunknown.sv (--allow-four-state-ops): expected isunknown op, got rc={ri.returncode}:\n{ri.stdout}")
    else:
        passed += 1

    for msg in failures:
        print(f"FAIL: {msg}")
    print(f"netlist_gate: passed={passed} failed={len(failures)}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
