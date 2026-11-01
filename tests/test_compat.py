#!/usr/bin/env python3
"""
The slang overlay (src/compat/, docs/compat.md):

- every override is off by default, --list-overrides names them all, and none
  is still registered once the linked slang has the fix it stands in for;
- an unknown profile or override id is an error;
- a simulator file-list option (-i) is an error until filelist-dialect accepts it;
- vendor-systasks makes $fsdbDumpvars and friends known, while a name passed to
  one that does not resolve is still reported;
- auto-uvm compiles uvm_pkg.sv from $UVM_HOME when the sources use UVM and do
  not list it; uvm-library then keeps warnings inside the UVM library out of
  the report;
- the config's compat section turns overrides on, the command line can turn
  one back off, and the JSON report lists the ones that were active.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
failures = []


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + detail.replace("\n", "\n      ")[:3000])


def run(binary, cwd, *args, env_extra=None):
    env = dict(os.environ)
    env.pop("UVM_HOME", None)
    env.update(env_extra or {})
    res = subprocess.run([binary, "--no-cache", *args], cwd=cwd, capture_output=True, text=True, env=env)
    return res.returncode, res.stdout, res.stderr


def report(binary, cwd, *args, env_extra=None):
    """Runs with a JSON report on stdout; returns (rc, report or None, stderr)."""
    rc, out, err = run(binary, cwd, "--json", "-", *args, env_extra=env_extra)
    try:
        return rc, json.loads(out), err
    except json.JSONDecodeError:
        return rc, None, out + err


def compile_messages(rep):
    return [v["message"] for v in (rep or {}).get("violations", []) if v["ruleId"] in ("SV-CMP-001", "SV-CMP-002")]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)

        print("Registry")
        rc, out, err = run(binary, root, "--list-overrides")
        for oid in ("filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks", "slang-compat-vcs"):
            check(rc == 0 and oid in out, f"--list-overrides names {oid}", out + err)
        check("questa" in out and "xcelium" in out and "vcs" in out, "--list-overrides names the profiles", out)
        # An override whose fix the linked slang already has must be deleted, not kept around.
        check("[retired" not in out, "no override has outlived the slang bug it covers", out)
        (root / "empty.sv").write_text("module empty; endmodule\n")
        rc, _, err = run(binary, root, "--no-config", "empty.sv", "--compat-profile", "bogus")
        check(rc == 2 and "unknown compat profile 'bogus'" in err, "unknown profile is an error", err)
        rc, _, err = run(binary, root, "--no-config", "empty.sv", "--override", "nope")
        check(rc == 2 and "unknown override 'nope'" in err, "unknown override is an error", err)
        rc, rep, err = report(binary, root, "--no-config", "empty.sv")
        check(rep is not None and "compatOverrides" not in rep, "nothing is on by default", err)

        print("filelist-dialect")
        (root / "inc").mkdir()
        (root / "inc" / "defs.svh").write_text("`define WIDTH 8\n")
        (root / "dut.sv").write_text('`include "defs.svh"\nmodule dut; logic [`WIDTH-1:0] d; endmodule\n')
        (root / "files.f").write_text("-i inc\ndut.sv\n")
        rc, _, err = run(binary, root, "--no-config", "-f", "files.f")
        check(rc == 2 and "'-i' is a simulator option" in err and "filelist-dialect" in err,
              "-i without the override names the override to turn on", err)
        rc, rep, err = report(binary, root, "--no-config", "-f", "files.f", "--override", "filelist-dialect")
        check(rep is not None and not compile_messages(rep), "-i adds an include directory", err or str(rep))

        print("vendor-systasks")
        (root / "dump.sv").write_text(
            "module top;\n  logic clk;\n  initial begin\n    $fsdbDumpfile(\"w.fsdb\");\n"
            "    $fsdbDumpvars(0, top);\n    $shm_probe(nosuch, \"AS\");\n    $vcdpluson;\n  end\nendmodule\n")
        rc, rep, err = report(binary, root, "--no-config", "dump.sv", "--rule", "SV-CMP-001")
        msgs = compile_messages(rep)
        check(any("$fsdbDumpvars" in m for m in msgs), "without the override $fsdbDumpvars is unknown", str(msgs))
        rc, rep, err = report(binary, root, "--no-config", "dump.sv", "--rule", "SV-CMP-001",
                              "--override", "vendor-systasks")
        msgs = compile_messages(rep)
        check(not any("unknown system name" in m for m in msgs), "vendor tasks are known with the override", str(msgs))
        check(any("'nosuch'" in m for m in msgs), "an unresolved name passed to one is still reported", str(msgs))
        check(rep is not None and rep.get("compatOverrides") == ["vendor-systasks"],
              "the JSON report lists the active override", str(rep and rep.get("compatOverrides")))

        print("auto-uvm and uvm-library")
        uvm = root / "uvm-core" / "src"
        uvm.mkdir(parents=True)
        (uvm / "uvm_macros.svh").write_text("`define uvm_info_stub(m) $display(m);\n")
        (uvm / "uvm_pkg.sv").write_text(
            'package uvm_pkg;\n  class uvm_object;\n    string sep = "\\.(";\n  endclass\nendpackage\n')
        tb = root / "tb"
        tb.mkdir()
        (tb / "test.sv").write_text(
            '`include "uvm_macros.svh"\nimport uvm_pkg::*;\nclass my_obj extends uvm_object;\nendclass\n'
            "module tb_top; initial `uvm_info_stub(\"hi\") endmodule\n")
        home = {"UVM_HOME": str(root / "uvm-core")}
        rc, rep, err = report(binary, tb, "--no-config", "test.sv", env_extra=home)
        check(rep is not None and any("uvm_pkg" in m or "uvm_macros" in m for m in compile_messages(rep)),
              "without the override UVM is missing", err or str(compile_messages(rep)))
        rc, rep, err = report(binary, tb, "--no-config", "test.sv", "--override", "auto-uvm", env_extra=home)
        msgs = compile_messages(rep)
        check(rep is not None and not any("uvm_pkg" in m or "uvm_macros" in m or "uvm_object" in m for m in msgs),
              "auto-uvm compiles uvm_pkg.sv from $UVM_HOME", err + str(msgs))
        check(any("escape" in m for m in msgs), "a warning inside the UVM library is reported without uvm-library",
              str(msgs))
        rc, rep, err = report(binary, tb, "--no-config", "test.sv", "--override", "auto-uvm",
                              "--override", "uvm-library", env_extra=home)
        check(rep is not None and not compile_messages(rep), "uvm-library keeps library warnings out of the report",
              err + str(compile_messages(rep)))
        rc, _, err = run(binary, tb, "--no-config", "test.sv", "--override", "auto-uvm")
        check("no uvm_pkg.sv was found" in err, "auto-uvm says when it cannot find UVM", err)

        print("Config")
        (tb / "cfg.json").write_text(json.dumps({
            "compat": {"profile": "questa", "uvm_home": str(root / "uvm-core")}}))
        rc, rep, err = report(binary, tb, "--config", "cfg.json", "test.sv")
        check(rep is not None and not compile_messages(rep) and
              rep.get("compatOverrides") == ["filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks"],
              "the config's compat profile and uvm_home apply", err + str(rep and rep.get("compatOverrides")))
        rc, rep, err = report(binary, tb, "--config", "cfg.json", "test.sv", "--override", "-uvm-library")
        check(rep is not None and "uvm-library" not in rep.get("compatOverrides", []) and
              any("escape" in m for m in compile_messages(rep)),
              "the command line turns an override back off", err + str(rep and rep.get("compatOverrides")))

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll compat checks passed.")


if __name__ == "__main__":
    main()
