#!/usr/bin/env python3
"""Each testdata/vendor probe is rejected by uvmcheck as written and accepted
with the slang option its first line names, so the measured tables
(tools/quality/vendor_table.py) measure the construct they say they do."""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def errors(binary, extra, f):
    res = subprocess.run([binary, "--no-config", "--no-cache", "--rule", "SV-CMP-001", *extra, str(f)],
                         capture_output=True, text=True)
    return (res.stdout + res.stderr).count("SV-CMP-001 (")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    binary = ap.parse_args().binary
    fails = 0
    probes = sorted((ROOT / "testdata" / "vendor").glob("*.sv"))
    for f in probes:
        flag = re.match(r"//\s*slang:\s*(\S+)", f.read_text().splitlines()[0]).group(1)
        plain = errors(binary, [], f)
        relaxed = 0 if flag == "none" else errors(binary, [flag], f)
        ok = plain > 0 and relaxed == 0
        print(f"  [{'PASS' if ok else 'FAIL'}] {f.name}: {plain} error(s) as written, {relaxed} with {flag}")
        fails += not ok
    print("vendor probes: " + ("PASSED" if not fails and probes else f"{fails} failure(s)"))
    return 1 if fails or not probes else 0


if __name__ == "__main__":
    sys.exit(main())
