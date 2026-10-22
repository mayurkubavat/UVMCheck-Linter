#!/usr/bin/env python3
"""
UVMCheck Engine Equivalence Check

Runs every input with all rules enabled under the reference engine
(--engine isolated: one walk per rule, sequential) and under the default
single-pass parallel engine at several thread counts, and asserts that the
findings are identical.

By default the inputs are every tests/rules/*/*/{bad,good}.sv file. Extra
inputs (for example a real UVM library) can be passed with --extra:

    tests/engine_equivalence.py --extra "uvm-core/src/uvm_pkg.sv -I uvm-core/src"
"""

import argparse
import collections
import concurrent.futures
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-9;]*m")
FINDING = re.compile(r"^\S.*:\d+:\d+: \[(ERROR|WARNING|INFO|STYLE)\] ")

# SV-OOP-013 also reports slang's own NewVirtualClass diagnostics, whose
# per-specialization counts vary from run to run inside slang itself (the
# same happens with the old engine), so it is compared by location only.
LOCATION_ONLY_RULES = {"SV-OOP-013"}


def findings(binary, args, engine, threads):
    cmd = [binary, *args, "--engine", engine]
    if threads:
        cmd += ["-j", str(threads)]
    res = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    counts = collections.Counter()
    for line in ANSI.sub("", res.stdout).splitlines():
        if not FINDING.match(line):
            continue
        rule = re.search(r"\] (\S+) \(", line)
        if rule and rule.group(1) in LOCATION_ONLY_RULES:
            counts[line] = 1
        else:
            counts[line] += 1
    return res.returncode, counts


def check(binary, args, thread_counts):
    ref_rc, ref = findings(binary, args, "isolated", None)
    problems = []
    for threads in thread_counts:
        rc, got = findings(binary, args, "single-pass", threads)
        if rc != ref_rc:
            problems.append(f"exit code {rc} != {ref_rc} (-j {threads})")
        if got != ref:
            missing = ref - got
            extra = got - ref
            detail = [f"  missing: {l}" for l in list(missing)[:3]] + \
                     [f"  extra:   {l}" for l in list(extra)[:3]]
            problems.append(f"-j {threads}: {sum(missing.values())} missing, "
                            f"{sum(extra.values())} extra\n" + "\n".join(detail))
    return sum(ref.values()), problems


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default="build/uvmcheck")
    parser.add_argument("--rules-dir", default="tests/rules")
    parser.add_argument("--common-dir", default="tests/common")
    parser.add_argument("--threads", default="1,4",
                        help="comma-separated thread counts for the single-pass engine")
    parser.add_argument("--extra", action="append", default=[],
                        help="extra command line (files and options) to check")
    args = parser.parse_args()

    binary = os.path.abspath(args.binary)
    thread_counts = [int(t) for t in args.threads.split(",")]
    common = str(Path(args.common_dir).resolve())

    cases = [[str(f), "-I", common] for f in sorted(Path(args.rules_dir).glob("*/*/*.sv"))]
    cases += [shlex.split(e) for e in args.extra]

    failures = 0
    total = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count()) as pool:
        futures = {pool.submit(check, binary, case, thread_counts): case for case in cases}
        for fut in concurrent.futures.as_completed(futures):
            case = futures[fut]
            count, problems = fut.result()
            total += count
            if problems:
                failures += 1
                print(f"[FAIL] {' '.join(case)}")
                for p in problems:
                    print("  " + p)

    print(f"Checked {len(cases)} inputs, {total} reference findings: "
          f"{'all identical' if not failures else f'{failures} input(s) differ'}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
