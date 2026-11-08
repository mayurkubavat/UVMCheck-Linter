#!/usr/bin/env python3
"""
Keeps rules on the single-pass engine.

A rule that walks the whole design itself (compilation.getRoot().visit(...) or a loop over
getSyntaxTrees()) inside run() costs a full extra traversal per rule. New rules hand their
visitors to the shared walk with check() / ctx.visitAst() / ctx.visitSyntax() instead (see
CONTRIBUTING.md). Rules that only query the UVM model (classes, methods) are fine in run().

cpp/tests/engine_legacy_allowlist.txt lists the rule files that still do their own full walk.
This test fails when a file not on the list starts doing one, and when a listed file no
longer does (so the list only shrinks).
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
ALLOWLIST = REPO / "cpp" / "tests" / "engine_legacy_allowlist.txt"
FULL_WALK = re.compile(r"getRoot\(\)\s*\.visit\(|getSyntaxTrees\(\)")
RUN_OVERRIDE = re.compile(r"\bvoid\s+run\s*\(")


def full_walk_files():
    found = set()
    for p in sorted((REPO / "cpp" / "src" / "rules").rglob("*.cpp")):
        text = p.read_text()
        if RUN_OVERRIDE.search(text) and FULL_WALK.search(text):
            found.add(p.relative_to(REPO).as_posix())
    return found


def main():
    if "--write" in sys.argv:
        files = sorted(full_walk_files())
        ALLOWLIST.write_text("# Rule files that still walk the whole design in run(); see cpp/tests/test_engine_gate.py.\n"
                             "# Migrate them to check() + ctx.visitAst/visitSyntax and delete the line.\n"
                             + "".join(f + "\n" for f in files))
        print(f"wrote {len(files)} entries")
        return 0
    allowed = {l.strip() for l in ALLOWLIST.read_text().splitlines() if l.strip() and not l.startswith("#")}
    actual = full_walk_files()
    failures = []
    for f in sorted(actual - allowed):
        failures.append(f"{f} walks the whole design in run(); use check() with ctx.visitAst/visitSyntax")
    for f in sorted(allowed - actual):
        failures.append(f"{f} no longer does its own walk; remove it from {ALLOWLIST.name}")
    for f in failures:
        print(f"FAIL: {f}")
    print(f"engine gate: {len(actual)} legacy full-walk rule files, {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
