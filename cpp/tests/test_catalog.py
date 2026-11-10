#!/usr/bin/env python3
"""
Checks the rule catalog generated from the rules' own metadata:

- every rule and sub-rule has a unique ID and a category from the closed set;
- every ID a rule reports appears in the catalog (no rule reports an undeclared ID);
- the binary declares the same metadata as docs/rules/catalog.json (generated from the
  native engine by tools/codegen/gen_rule_docs.py) for every rule not marked native there.
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

    # The catalog is generated from the native engine's rules
    # (tools/codegen/gen_rule_docs.py). This engine must declare the same
    # metadata for every rule it has; rules marked "engine": "native" are the
    # native engine's alone.
    published = json.loads((REPO / "docs" / "rules" / "catalog.json").read_text())
    if published["categories"] != catalog["categories"]:
        failures.append("categories differ from docs/rules/catalog.json")
    shared = {r["id"]: r for r in published["rules"] if r.get("engine") != "native"}
    mine = {r["id"]: r for r in rules}
    for rid in sorted(set(shared) | set(mine)):
        if rid not in mine:
            failures.append(f"{rid}: in docs/rules/catalog.json, not registered here (native only? mark it so)")
        elif rid not in shared:
            failures.append(f"{rid}: registered here, missing from docs/rules/catalog.json or marked native there")
        else:
            for key in sorted(set(shared[rid]) | set(mine[rid])):
                if shared[rid].get(key) != mine[rid].get(key):
                    failures.append(f"{rid}: {key} is {mine[rid].get(key)!r} here, "
                                    f"{shared[rid].get(key)!r} in docs/rules/catalog.json")

    for f in failures:
        print(f"FAIL: {f}")
    print(f"rule catalog: {len(rules)} entries, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
