"""
Test harness for Python rules, using the same layout as the built-in rule tests:

    <tests>/<RULE-ID>/bad.sv       must produce exactly the findings in expect.json
    <tests>/<RULE-ID>/good.sv      must produce no finding of RULE-ID
    <tests>/<RULE-ID>/expect.json  {"bad.sv": [{"rule": "RULE-ID", "line": 12}, ...]}

Run:
    python3 -m uvmcheck.testing --rules my_rules/ --tests my_rule_tests/ [--binary uvmcheck]
                                [-- extra uvmcheck arguments, e.g. -I include]
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import List, Tuple


def lint(binary: str, rules: str, source: Path, extra: List[str]) -> List[Tuple[str, int]]:
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "report.json"
        res = subprocess.run([binary, "--python-rules", rules, "--json", str(out), *extra, str(source)],
                             capture_output=True, text=True)
        if not out.exists():
            raise RuntimeError(f"uvmcheck failed on {source}:\n{res.stdout}{res.stderr}")
        if "Python rule" in res.stderr and "failed" in res.stderr:
            raise RuntimeError(f"Python rules failed on {source}:\n{res.stderr}")
        report = json.loads(out.read_text())
    return [(v["ruleId"], v["line"]) for v in report["violations"]]


def run_tests(binary: str, rules: str, tests: Path, extra: List[str]) -> int:
    failures = 0
    dirs = sorted(d for d in tests.iterdir() if d.is_dir() and (d / "bad.sv").exists())
    for d in dirs:
        rule_id = d.name
        expect = json.loads((d / "expect.json").read_text()) if (d / "expect.json").exists() else {}
        wanted = sorted((e["rule"], e["line"]) for e in expect.get("bad.sv", []))
        try:
            got = sorted(f for f in lint(binary, rules, d / "bad.sv", extra) if f[0] == rule_id)
            ok_bad = got == wanted and bool(got)
            print(f"[{'PASS' if ok_bad else 'FAIL'}] {rule_id}/bad.sv"
                  + ("" if ok_bad else f": expected {wanted}, got {got}"))
            failures += not ok_bad
            if (d / "good.sv").exists():
                got = [f for f in lint(binary, rules, d / "good.sv", extra) if f[0] == rule_id]
                print(f"[{'PASS' if not got else 'FAIL'}] {rule_id}/good.sv" + ("" if not got else f": got {got}"))
                failures += bool(got)
        except RuntimeError as e:
            print(f"[FAIL] {rule_id}: {e}")
            failures += 1
    print(f"{len(dirs)} rule test(s), {failures} failure(s)")
    return 1 if failures or not dirs else 0


def main(argv=None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    extra = []
    if "--" in argv:
        i = argv.index("--")
        argv, extra = argv[:i], argv[i + 1:]
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--rules", required=True, help="Python rule file or directory")
    ap.add_argument("--tests", required=True, help="directory of <RULE-ID>/ test folders")
    ap.add_argument("--binary", default=os.environ.get("UVMCHECK", shutil.which("uvmcheck") or "uvmcheck"))
    args = ap.parse_args(argv)
    return run_tests(args.binary, str(Path(args.rules).resolve()), Path(args.tests), extra)


if __name__ == "__main__":
    sys.exit(main())
