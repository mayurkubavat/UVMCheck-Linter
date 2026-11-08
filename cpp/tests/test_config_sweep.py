#!/usr/bin/env python3
"""scripts/config_sweep.py finds a bug that only compiles under an `ifdef."""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FAILS = 0


def check(cond, msg, detail=""):
    global FAILS
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        FAILS += 1
        if detail:
            print("        " + detail.replace("\n", "\n        "))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    binary = str(Path(ap.parse_args().binary).resolve())
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "defs.svh").write_text("`define HAVE_DEFS\n")
        (root / "dut.sv").write_text(
            '`include "defs.svh"\n'
            "module sweep_dut(input logic [1:0] sel, output logic [7:0] q);\n"
            "  always_comb begin\n"
            "`ifdef FAST_PATH\n"
            "    casex (sel)\n"
            "`else\n"
            "    case (sel)\n"
            "`endif\n"
            "      2'b00: q = 8'h01;\n"
            "      default: q = 8'h00;\n"
            "    endcase\n"
            "`ifdef HAVE_DEFS\n"
            "`endif\n"
            "  end\n"
            "endmodule\n")
        cmd = [sys.executable, str(ROOT / "scripts" / "config_sweep.py"), "--binary", binary, "--auto",
               "--set", "FAST_PATH,WIDTH=4", "--json", "out.json", "--",
               "--no-config", "--no-cache", "--rule", "SV-COR-009", "-I", ".", "dut.sv"]
        res = subprocess.run(cmd, cwd=root, capture_output=True, text=True)
        out = json.loads((root / "out.json").read_text()) if (root / "out.json").exists() else {}
        names = [c["name"] for c in out.get("configurations", [])]
        check(names == ["default", "FAST_PATH,WIDTH=4", "FAST_PATH"],
              "--set and --auto configurations, minus macros the sources define", str(names) + res.stderr)
        hits = [v for v in out.get("violations", []) if v["ruleId"] == "SV-COR-009"]
        check(len(hits) == 1 and hits[0]["line"] == 5 and "default" not in hits[0].get("configurations", []),
              "the casex inside `ifdef FAST_PATH is found, tagged with its configurations", str(hits))
        check(res.returncode == 1 and "1 only outside the default configuration" in res.stdout,
              "summary names it; the error-severity finding makes the exit status 1", res.stdout + res.stderr)
    print("config sweep: " + ("PASSED" if not FAILS else f"{FAILS} failure(s)"))
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
