#!/usr/bin/env python3
"""
uvmcheck.report: merge, diff and trend over JSON reports. Findings keep
their identity when lines move (fingerprints), so a diff after an edit
shows only what really changed.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
failures = []

V1 = """module blk_a;
  int a, b;
  initial begin
    if (a == a) $display("same");
    if (b == b) $display("same");
  end
endmodule : blk_a
"""
# One line inserted at the top, the b==b finding fixed, a new c==c finding.
V2 = """// moved down by one line
module blk_a;
  int a, b, c;
  initial begin
    if (a == a) $display("same");
    if (b == 1) $display("fixed");
    if (c == c) $display("new");
  end
endmodule : blk_a
"""
OTHER = """module blk_b;
  int d;
  initial if (d == d) $display("other");
endmodule : blk_b
"""


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + str(detail)[:2000].replace("\n", "\n      "))


def lint(binary, cwd, src, out):
    subprocess.run([binary, src, "--no-config", "--rule", "SV-COR-001", "--json", out], cwd=cwd,
                   capture_output=True, text=True)
    return json.loads(Path(cwd, out).read_text())


def report(*args):
    return subprocess.run([sys.executable, "-m", "uvmcheck.report", *args], capture_output=True, text=True,
                          cwd=REPO / "python")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="build/uvmcheck")
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())
    print("[REPORT TEST] uvmcheck.report merge / diff / trend")

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "rtl" / "a").mkdir(parents=True)
        (d / "rtl" / "b").mkdir(parents=True)
        (d / "rtl/b/b.sv").write_text(OTHER)
        (d / "rtl/a/a.sv").write_text(V1)
        r1 = lint(binary, d, "rtl/a/a.sv", "r1.json")
        check(len(r1["violations"]) == 2 and all("fingerprint" in v and "category" in v for v in r1["violations"]),
              "JSON findings carry a fingerprint and a category", r1)
        lint(binary, d, "rtl/b/b.sv", "rb.json")
        (d / "rtl/a/a.sv").write_text(V2)
        lint(binary, d, "rtl/a/a.sv", "r2.json")

        res = report("diff", str(d / "r1.json"), str(d / "r2.json"), "--json", str(d / "diff.json"))
        dj = json.loads((d / "diff.json").read_text())
        check(len(dj["new"]) == 1 and dj["new"][0]["line"] == 7, "diff: one new finding (c == c)", res.stdout)
        check(len(dj["fixed"]) == 1 and dj["fixed"][0]["line"] == 5, "diff: one fixed finding (b == b)", res.stdout)
        check(dj["unchanged"] == 1, "diff: the moved a == a finding is unchanged", res.stdout)
        check("1 new, 1 fixed, 1 unchanged" in res.stdout, "diff prints a summary", res.stdout)
        res = report("diff", str(d / "r1.json"), str(d / "r2.json"), "--fail-on-new")
        check(res.returncode == 1, "--fail-on-new exits 1 when something is new")
        res = report("diff", str(d / "r2.json"), str(d / "r2.json"), "--fail-on-new")
        check(res.returncode == 0, "--fail-on-new exits 0 when nothing is new")

        res = report("merge", str(d / "r1.json"), str(d / "rb.json"), str(d / "r1.json"), "-o", str(d / "m.json"))
        m = json.loads((d / "m.json").read_text())
        check(len(m["violations"]) == 3 and m["mergedFrom"] == 3, "merge keeps each finding once", res.stdout)

        res = report("trend", str(d / "r1.json"), str(d / "r2.json"), "-o", str(d / "t.html"), "--labels", "mon,tue")
        page = (d / "t.html").read_text() if (d / "t.html").exists() else ""
        check(res.returncode == 0 and "Lint trend" in page and "rtl/a" in page and "Correctness" in page,
              "trend page by category and block", res.stdout + res.stderr)

        res = report("diff", str(d / "r1.json"), str(d / "missing.json"))
        check(res.returncode == 2 and "Error" in res.stderr, "missing report is an error (exit 2)")

    if failures:
        print(f"{len(failures)} check(s) failed")
        return 1
    print("All report checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
