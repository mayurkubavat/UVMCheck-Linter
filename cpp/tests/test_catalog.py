#!/usr/bin/env python3
"""
Checks the rule catalog generated from the rules' own metadata:

- every rule and sub-rule has a unique ID and a category from the closed set;
- every ID a rule reports appears in the catalog (no rule reports an undeclared ID);
- docs/rules/catalog.json, docs/rules/README.md and the methodology "Enforced by" lines
  match what tools/codegen/gen_rule_docs.py generates from the binary.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())
    failures = []

    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "catalog.json"
        res = subprocess.run([binary, "--list-rules", "--json", str(out)], capture_output=True,
                             text=True, cwd=tmp)
        if res.returncode != 0:
            sys.exit(f"--list-rules --json failed: {res.stderr}")
        catalog = json.loads(out.read_text())

    rules = catalog["rules"]
    categories = set(catalog["categories"])
    dups = [i for i, n in Counter(r["id"] for r in rules).items() if n > 1]
    if dups:
        failures.append(f"duplicate rule IDs: {dups}")
    for r in rules:
        if r["category"] not in categories:
            failures.append(f"{r['id']}: category {r['category']!r} is not one of {sorted(categories)}")
        if not r["description"].strip():
            failures.append(f"{r['id']}: empty description")

    # IDs that rule tests expect must be declared in the catalog.
    known = {r["id"] for r in rules}
    for expect in (REPO / "testdata" / "rules").glob("*/*/expect.json"):
        for findings in json.loads(expect.read_text()).values():
            for f in findings:
                if f["rule"] not in known:
                    failures.append(f"{expect.relative_to(REPO)} expects undeclared rule {f['rule']}")

    res = subprocess.run([sys.executable, str(REPO / "tools" / "codegen" / "gen_rule_docs.py"), "--binary", binary,
                          "--check"], capture_output=True, text=True)
    if res.returncode != 0:
        failures.append(res.stdout.strip() or res.stderr.strip())

    for f in failures:
        print(f"FAIL: {f}")
    print(f"rule catalog: {len(rules)} entries, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
