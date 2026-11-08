#!/usr/bin/env python3
"""
tools/quality/precision.py: measures findings per rule per kLOC on a corpus and
flags rules whose counts jump against a previous result.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
failures = []

CODE = """module p_tb;
  int a, b;
  initial begin
    if (a == a) $display("x");
  end
endmodule : p_tb
"""


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + str(detail)[:2000].replace("\n", "\n      "))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="build/uvmcheck")
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())
    print("[PRECISION TEST] tools/quality/precision.py")

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "src").mkdir()
        (d / "src/tb.sv").write_text(CODE)
        corpus = [{"name": "demo", "args": ["${PREC_HOME}/src/tb.sv"], "count": ["${PREC_HOME}/src"]},
                  {"name": "unset", "args": ["${PREC_UNSET_VAR}/x.sv"], "count": []}]
        (d / "corpus.json").write_text(json.dumps(corpus))
        env = {**os.environ, "PREC_HOME": str(d)}
        env.pop("PREC_UNSET_VAR", None)
        env.pop("GITHUB_STEP_SUMMARY", None)

        def run(out, *extra):
            return subprocess.run([sys.executable, str(REPO / "tools/quality/precision.py"), "--binary", binary,
                                   "--corpus", str(d / "corpus.json"), "--out", str(d / out), *extra],
                                  capture_output=True, text=True, env=env, cwd=d)

        res = run("first.json", "--summary", str(d / "s.md"))
        first = json.loads((d / "first.json").read_text())
        demo = first["corpora"].get("demo", {})
        check(res.returncode == 0 and demo.get("rules", {}).get("SV-COR-001") == 1, "counts findings per rule",
              res.stdout + res.stderr)
        check("unset" not in first["corpora"] and "skipping unset" in res.stdout, "corpus with unset variable skipped")
        check(demo.get("kloc", 0) > 0 and "per kLOC" in (d / "s.md").read_text(), "summary with per-kLOC rates")

        # Pretend the previous run had far fewer SV-COR-001 findings, and one rule that now reports none.
        prev = json.loads((d / "first.json").read_text())
        prev["corpora"]["demo"]["rules"] = {**demo["rules"], "SV-COR-001": 0, "SV-FAKE-001": 9}
        prev["corpora"]["demo"]["rules"]["SV-COR-001"] = 0
        (d / "prev.json").write_text(json.dumps(prev))
        (d / "src/tb.sv").write_text(CODE.replace('    if (a == a) $display("x");\n',
                                                  '    if (a == a) $display("x");\n' * 6))
        res = run("second.json", "--previous", str(d / "prev.json"), "--fail-on-jump")
        flags = {f[0]: f[3] for f in json.loads((d / "second.json").read_text())["flags"]["demo"]}
        check(flags.get("SV-COR-001") == "jumped", "jump flagged", flags)
        check(flags.get("SV-FAKE-001") == "dropped to zero", "drop to zero flagged", flags)
        check(res.returncode == 1, "--fail-on-jump exits 1")

    if failures:
        print(f"{len(failures)} check(s) failed")
        return 1
    print("All precision checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
