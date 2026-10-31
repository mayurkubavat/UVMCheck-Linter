#!/usr/bin/env python3
"""
--changed-since <ref>: only findings on lines added or changed since the
revision are reported, plus everything in untracked files; an unknown
revision or a folder outside git is an error (exit 2).
"""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
failures = []

OLD = """module tb;
  int a, b;
  initial begin
    if (a == a) $display("old");
  end
endmodule : tb
"""
NEW = OLD.replace('    if (a == a) $display("old");\n',
                  '    if (a == a) $display("old");\n    if (b == b) $display("new");\n')


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + detail[:2000].replace("\n", "\n      "))


def git(cwd, *args):
    subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@t", *args], cwd=cwd, check=True,
                   capture_output=True)


def lint(binary, cwd, *args):
    res = subprocess.run([binary, "--no-config", *args], cwd=cwd, capture_output=True, text=True)
    out = re.sub(r"\x1b\[[0-9;]*m", "", res.stdout + res.stderr)
    found = re.findall(r"^(\S+?):(\d+):\d+: \[\w+\] (SV-COR-008)", out, re.M)
    return res.returncode, [(Path(f).name, int(l)) for f, l, _ in found], out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "tb.sv").write_text(OLD)
        git(root, "init", "-q")
        git(root, "add", "tb.sv")
        git(root, "commit", "-q", "-m", "base")
        (root / "tb.sv").write_text(NEW)
        (root / "extra.sv").write_text(OLD.replace("module tb", "module extra").replace(": tb", ": extra"))

        rc, found, out = lint(binary, root, "tb.sv", "extra.sv")
        check(sorted(found) == [("extra.sv", 4), ("tb.sv", 4), ("tb.sv", 5)], "without the flag: every finding", out)

        rc, found, out = lint(binary, root, "tb.sv", "extra.sv", "--changed-since", "HEAD")
        check(sorted(found) == [("extra.sv", 4), ("tb.sv", 5)],
              "only the changed line, plus the untracked file", out)
        check("not shown" in out, "summary says how many were left out", out)

        rc, found, out = lint(binary, root, "tb.sv", "--changed-since", "no-such-ref")
        check(rc == 2 and "unknown git revision" in out, "unknown revision is an error", out)

    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "tb.sv").write_text(OLD)
        rc, found, out = lint(binary, tmp, "tb.sv", "--changed-since", "HEAD")
        check(rc == 2 and "not inside a git repository" in out, "outside git is an error", out)

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll --changed-since checks passed.")


if __name__ == "__main__":
    main()
