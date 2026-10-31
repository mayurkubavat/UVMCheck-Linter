#!/usr/bin/env python3
"""
--fix and --fix-dry-run: the dry run prints a unified diff and changes
nothing; the diff applies cleanly and gives the same text as --fix; fixes in
an included file land in that file; a second --fix finds nothing to do.
"""

import argparse
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

failures = []

TOP = """`include "helpers.svh"
module fix_demo;
    logic [3:0] sel;
    logic [1:0] out;
    logic a, y;
    always_comb begin
        casex (sel)
            4'b1xx0: out = 2'b00;
            default: out = 2'b10;
        endcase
    end
    always_comb begin
        if (a == 1'bx) y = 0;
        else y = 1;
    end
endmodule : fix_demo
"""
HELPER = """module fix_helper;
    logic b, z;
    always_comb begin
        if (b != 1'bz) z = 0;
        else z = 1;
    end
endmodule : fix_helper
"""


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + detail[:2000].replace("\n", "\n      "))


def run(binary, cwd, *args):
    return subprocess.run([binary, "top.sv", "--no-config", *args], cwd=cwd, capture_output=True, text=True)


def setup(d):
    (d / "top.sv").write_text(TOP)
    (d / "helpers.svh").write_text(HELPER)


def extract_diff(stdout):
    lines, inside = [], False
    for line in stdout.splitlines():
        if line.startswith("--- a/"):
            inside = True
        if line.startswith("[uvmcheck]"):
            inside = False
        if inside:
            lines.append(line)
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="build/uvmcheck")
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())
    print("[FIX TEST] --fix / --fix-dry-run")

    with tempfile.TemporaryDirectory() as tmp:
        dry, real = Path(tmp, "dry"), Path(tmp, "real")
        dry.mkdir()
        real.mkdir()
        setup(dry)
        setup(real)

        res = run(binary, dry, "--fix-dry-run")
        check((dry / "top.sv").read_text() == TOP and (dry / "helpers.svh").read_text() == HELPER,
              "dry run changes no file")
        check("Would apply 3 fix-it(s) in 2 file(s)" in res.stdout, "dry run counts the fixes", res.stdout)
        diff = extract_diff(res.stdout)
        check("-        casex (sel)" in diff and "+        casez (sel)" in diff, "diff shows the rewrite", diff)

        res = run(binary, real, "--fix")
        check("Applied 3 fix-it(s) in 2 file(s)" in res.stdout, "--fix applies the fixes", res.stdout)
        top, helper = (real / "top.sv").read_text(), (real / "helpers.svh").read_text()
        check("casez (sel)" in top and "a === 1'bx" in top, "fixes applied in the top file", top)
        check("b !== 1'bz" in helper, "fix applied in the included file", helper)
        check(not list(real.glob("*.tmp")), "no temporary files left behind")

        if shutil.which("patch"):
            p = subprocess.run(["patch", "-p1", "-s"], input=diff, cwd=dry, capture_output=True, text=True)
            check(p.returncode == 0, "diff applies with patch", p.stdout + p.stderr)
            check((dry / "top.sv").read_text() == top and (dry / "helpers.svh").read_text() == helper,
                  "patched files match --fix")

        res = run(binary, real, "--fix")
        check("fix-it" not in res.stdout, "second --fix has nothing left to do", res.stdout)

    if failures:
        print(f"{len(failures)} check(s) failed")
        return 1
    print("All fix checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
