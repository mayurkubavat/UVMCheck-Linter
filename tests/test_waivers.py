#!/usr/bin/env python3
"""
Waiver hygiene (--waiver-report, --strict-waivers):

- a config waiver that matches no finding is listed as unused; one that does
  is counted under its owner;
- an expired waiver is listed;
- a waiver comment that waives nothing is listed with file:line; one that does
  waive a finding is not;
- baseline entries whose finding is gone are counted;
- --strict-waivers exits 4 while any of these remain, 0 once they are cleaned up.
"""

import argparse
import re
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
failures = []

SOURCE = """module user_tb;
  int a, b;
  initial begin
    if (a == a) $display("one");   // uvmcheck:waive SV-COR-008 -- reviewed
    if (b == b) $display("two");
    $display("clean");             // uvmcheck:waive SV-COR-008 -- stale comment
  end
endmodule : user_tb
"""

CONFIG = """\
from uvmcheck.config import ProjectConfig, Waiver

class Project(ProjectConfig):
    waivers = [
        Waiver("SV-COR-008", path="user_tb.sv", line=5, reason="known", owner="alice"),
        Waiver("SV-COR-001", path="user_tb.sv", reason="same finding as SV-COR-008", owner="bob"),
        Waiver("SV-COR-009", path="user_tb.sv", reason="nothing here", owner="carol"),
        Waiver("SV-COR-008", path="user_tb.sv", reason="old", expires="2001-01-01"),
    ]
"""


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print(textwrap.indent(detail[:3000], "      "))


def run(binary, cwd, *args):
    res = subprocess.run([binary, "user_tb.sv", *args], cwd=cwd, capture_output=True, text=True)
    strip = lambda s: re.sub(r"\x1b\[[0-9;]*m", "", s)
    return res.returncode, strip(res.stdout) + strip(res.stderr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / ".uvmcheck").mkdir()
        (root / "user_tb.sv").write_text(SOURCE)
        (root / ".uvmcheck" / "config.py").write_text(CONFIG)

        print("Report")
        rc, out = run(binary, root, "--waiver-report")
        check(rc == 0, "report alone does not change the exit status", out)
        check("Waivers: 3 written, 1 matched nothing, 1 expired" in out, "counts written/unused/expired", out)
        check(re.search(r"unused:\s+Waiver\(SV-COR-009\)", out) is not None, "unused config waiver named", out)
        check("expired: Waiver(SV-COR-008) expired 2001-01-01" in out, "expired waiver named", out)
        check("Waiver comments: 2 in source, 1 waive nothing" in out, "comment counts", out)
        check("user_tb.sv:6:" in out and "user_tb.sv:4:" not in out.split("Waiver comments")[1].split("Baseline")[0],
              "only the stale comment is listed", out)
        check("alice 1" in out and "bob 2" in out and "(inline comment) 1" in out, "waived findings by owner", out)

        rc, out = run(binary, root, "--strict-waivers")
        check(rc == 4, f"--strict-waivers exits 4 (got {rc})", out)

        print("Baseline")
        rc, out = run(binary, root, "--update-baseline", "base.json")
        (root / "user_tb.sv").write_text(SOURCE.replace('if (b == b) $display("two");', '$display("two");'))
        rc, out = run(binary, root, "--baseline", "base.json", "--waiver-report")
        check(re.search(r"Baseline: \d+ entr(y|ies), [1-9]\d* no longer match", out) is not None,
              "stale baseline entries counted", out)

        print("Clean")
        (root / "user_tb.sv").write_text(SOURCE.replace("             // uvmcheck:waive SV-COR-008 -- stale comment", ""))
        (root / ".uvmcheck" / "config.py").write_text(CONFIG.replace(
            '        Waiver("SV-COR-009", path="user_tb.sv", reason="nothing here", owner="carol"),\n', "").replace(
            '        Waiver("SV-COR-008", path="user_tb.sv", reason="old", expires="2001-01-01"),\n', ""))
        rc, out = run(binary, root, "--strict-waivers")
        check(rc == 0, f"--strict-waivers passes once cleaned up (got {rc})", out)

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll waiver hygiene checks passed.")


if __name__ == "__main__":
    main()
