#!/usr/bin/env python3
"""--explain: description, guidelines, options and the tested bad/good examples."""

import argparse
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
failures = []


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        print("      " + detail[:1500])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())
    run = lambda *a: subprocess.run([binary, *a], capture_output=True, text=True)

    r = run("--explain", "SV-DUP-002")
    out = r.stdout
    bad = (REPO / "testdata/rules/methodology/SV-DUP-002/bad.sv").read_text().splitlines()[1]
    check(r.returncode == 0 and "CloneFragments" in out, "rule found by ID", out)
    check("MG-7.1" in out, "guideline listed", out)
    check("min_tokens = 50" in out, "option and its default listed", out)
    check("testdata/rules/methodology/SV-DUP-002/bad.sv" in out and bad.strip() in out, "tested bad example shown", out)
    check("good.sv" in out, "good example shown", out)

    r = run("--explain", "CloneFragments")
    check(r.returncode == 0 and "SV-DUP-002" in r.stdout, "rule found by name", r.stdout)

    r = run("--explain", "UVM-NAM-101")
    check(r.returncode == 0 and "Reported by" in r.stdout, "finding ID of a convention rule explained", r.stdout)

    r = run("--explain", "NO-SUCH-001")
    check(r.returncode == 2 and "unknown rule" in r.stderr, "unknown rule is an error", r.stderr)

    if failures:
        sys.exit(1)
    print("\nAll --explain checks passed.")


if __name__ == "__main__":
    main()
