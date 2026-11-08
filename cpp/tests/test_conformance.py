#!/usr/bin/env python3
"""
IEEE 1800-2023 conformance corpus (testdata/conformance/, docs/conformance.md).

Every clause of the LRM from 5 to 35, and the 2023 additions, has a probe file
of legal SystemVerilog (or is marked out of scope in clauses.json). Each probe
is linted with every rule on, the way a user runs uvmcheck:

- the run must end normally (no crash, exit 0 or 1);
- the JSON report must parse;
- there must be no compile error (SV-CMP-001): the code is legal, so an error
  means slang, the overlay or our setup rejects valid SystemVerilog.

Findings from other rules are allowed (probes are not written to be clean), and
are printed so a new false positive on a construct is easy to spot.
"""

import argparse
import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CORPUS = REPO / "testdata" / "conformance"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    ap.add_argument("--verbose", action="store_true", help="list the findings of each probe")
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())

    failures = []
    manifest = json.loads((CORPUS / "clauses.json").read_text())["clauses"]
    expected = [str(c) for c in range(5, 36)] + ["2023"]
    for clause in expected:
        entry = manifest.get(clause)
        if entry is None:
            failures.append(f"clause {clause} has no probe and is not marked out of scope")
        elif isinstance(entry, str) and not (CORPUS / entry).is_file():
            failures.append(f"clause {clause}: probe {entry} does not exist")
        elif isinstance(entry, dict) and not entry.get("out_of_scope"):
            failures.append(f"clause {clause}: an entry without a file needs an out_of_scope reason")

    probes = sorted({e for e in manifest.values() if isinstance(e, str)})
    listed = {CORPUS / p for p in probes}
    for f in sorted((CORPUS / "lrm").glob("*.sv")):
        if f not in listed:
            failures.append(f"{f.relative_to(CORPUS)} is not listed in clauses.json")

    print(f"{'probe':40} {'rc':>3} {'findings':>8}  rules")
    for probe in probes:
        path = CORPUS / probe
        res = subprocess.run([binary, "--no-config", "--no-cache", "--preset", "all", "--json", "-", str(path)],
                             capture_output=True, text=True, cwd=CORPUS)
        if res.returncode < 0 or res.returncode > 1:
            failures.append(f"{probe}: exit {res.returncode}\n{res.stderr[-2000:]}")
            print(f"{probe:40} {res.returncode:>3}  CRASH or ERROR")
            continue
        try:
            report = json.loads(res.stdout)
        except json.JSONDecodeError:
            failures.append(f"{probe}: JSON report does not parse\n{res.stdout[:500]}{res.stderr[-1000:]}")
            continue
        violations = report.get("violations", [])
        errors = [v for v in violations if v["ruleId"] == "SV-CMP-001"]
        for e in errors:
            failures.append(f"{probe}:{e.get('line')}: compile error on legal code: {e['message']}")
        rules = Counter(v["ruleId"] for v in violations)
        print(f"{probe:40} {res.returncode:>3} {len(violations):>8}  " +
              ", ".join(f"{r}x{n}" if n > 1 else r for r, n in sorted(rules.items())))
        if args.verbose:
            for v in violations:
                print(f"      {v.get('line')}: {v['ruleId']} {v['message'][:140]}")

    print()
    if failures:
        print(f"FAILED ({len(failures)}):")
        for f in failures:
            print("  - " + f.replace("\n", "\n    "))
        return 1
    print(f"PASSED: {len(probes)} probes cover clauses 5-35 and the 2023 additions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
