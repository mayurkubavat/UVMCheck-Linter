#!/usr/bin/env python3
"""
Python rule API checks:

- the sample rules in tests/python_rules pass their bad/good tests through uvmcheck.testing,
  run from outside the repository (the SDK is found wherever uvmcheck runs);
- a rule customized by subclassing replaces its base (only the leaf runs);
- rule options come from the project config (RuleConfig attributes);
- a rule that raises does not hide the findings of the others.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CASES = REPO / "tests" / "python_rules" / "cases"
RULES = REPO / "tests" / "python_rules" / "rules"
COMMON = REPO / "testdata" / "common"

failures = []


def check(cond, msg):
    print(f"  {'ok' if cond else 'FAIL'}: {msg}")
    if not cond:
        failures.append(msg)


def lint(binary, cwd, rules, source, *extra):
    out = Path(cwd) / "report.json"
    res = subprocess.run([binary, "--python-rules", str(rules), "--json", str(out), "-I", str(COMMON),
                          *extra, str(source)], cwd=cwd, capture_output=True, text=True)
    return json.loads(out.read_text())["violations"], res.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        env = dict(os.environ, PYTHONPATH=str(REPO / "python"))
        res = subprocess.run([sys.executable, "-m", "uvmcheck.testing", "--binary", binary, "--rules", str(RULES),
                              "--tests", str(CASES), "--", "-I", str(COMMON)],
                             cwd=tmp, env=env, capture_output=True, text=True)
        print(res.stdout)
        check(res.returncode == 0, "sample rules pass their tests via uvmcheck.testing")

        found, _ = lint(binary, tmp, RULES, CASES / "PY-OVR-001" / "bad.sv")
        ovr = [v for v in found if v["ruleId"] == "PY-OVR-001"]
        check(len(ovr) == 1 and ovr[0]["message"].startswith("strict:") and ovr[0]["severity"] == "ERROR",
              "the subclassed rule runs instead of its base")

        proj = Path(tmp) / "proj"
        (proj / ".uvmcheck").mkdir(parents=True)
        (proj / ".uvmcheck" / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, RuleConfig\n"
            "class Project(ProjectConfig):\n"
            "    class UnreadConfigKey(RuleConfig):\n"
            "        rule = 'PY-CFG-001'\n"
            "        ignore = 'retries'\n")
        found, _ = lint(binary, proj, RULES, CASES / "PY-CFG-001" / "bad.sv")
        check(not [v for v in found if v["ruleId"] == "PY-CFG-001"], "rule option from config.py is applied")

        broken = Path(tmp) / "broken_rules"
        broken.mkdir()
        (broken / "a.py").write_text((RULES / "sample_rules.py").read_text())
        (broken / "b.py").write_text(
            "from uvmcheck import Rule\n"
            "class Boom(Rule):\n"
            "    id = 'PY-BOOM-001'\n"
            "    def analyze(self, ctx):\n"
            "        print('rules may print without breaking the report')\n"
            "        raise ValueError('boom')\n")
        found, err = lint(binary, tmp, broken, CASES / "PY-CFG-001" / "bad.sv")
        check(any(v["ruleId"] == "PY-CFG-001" for v in found), "other rules still report when one raises")
        check("boom" in err, "the failing rule's traceback is shown")

        fixing = Path(tmp) / "fix_rules"
        fixing.mkdir()
        (fixing / "fix.py").write_text(
            "from uvmcheck import Fix, FixSafety, Rule\n"
            "class WithFix(Rule):\n"
            "    id = 'PY-FIX-001'\n"
            "    def analyze(self, ctx):\n"
            "        self.report(ctx, 'needs a rewrite', filename='bad.sv', line=1,\n"
            "                    fixes=[Fix('casez', description='show casex', safety=FixSafety.Unsafe)])\n")
        src = Path(tmp) / "bad.sv"
        src.write_text("module m; endmodule\n")
        found, _ = lint(binary, tmp, fixing, src)
        hit = [v for v in found if v["ruleId"] == "PY-FIX-001"]
        check(len(hit) == 1 and hit[0].get("fixes") and hit[0]["fixes"][0]["safety"] == "unsafe"
              and hit[0]["fixes"][0]["replacement"] == "casez",
              "a Python rule's fix safety reaches the JSON report")

    print(f"python rule tests: {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
