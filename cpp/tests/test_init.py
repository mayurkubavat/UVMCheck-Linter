#!/usr/bin/env python3
"""
`uvmcheck --init`:

- finds the top-level sources (files nobody `includes) and skips headers;
- learns naming conventions the code follows instead of the defaults
  (drivers ending in _bfm, agents in _agt), and leaves the defaults where the
  code already follows them (items ending in _item);
- writes .uvmcheck/config.py and a baseline, after which a plain run is clean;
- prefers a readable .f file list over the discovered files;
- refuses to overwrite an existing config.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
failures = []


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + detail.replace("\n", "\n      ")[:3000])


def make_project(root: Path):
    agents = root / "tb" / "agents"
    agents.mkdir(parents=True)
    shutil.copy(REPO / "testdata" / "common" / "uvm_mock.svh", root / "tb" / "uvm_mock.svh")
    pkg = ['`include "uvm_mock.svh"']
    for a in ["axi", "apb", "spi"]:
        (agents / f"{a}_item.svh").write_text(
            f"class {a}_item extends uvm_sequence_item;\n"
            f"  `uvm_object_utils({a}_item)\n"
            f"  function new(string name = \"{a}_item\"); super.new(name); endfunction\n"
            f"endclass : {a}_item\n")
        (agents / f"{a}_bfm.svh").write_text(
            f"class {a}_bfm extends uvm_driver #({a}_item);\n"
            f"  `uvm_component_utils({a}_bfm)\n"
            f"  function new(string name, uvm_component parent); super.new(name, parent); endfunction\n"
            f"endclass : {a}_bfm\n")
        (agents / f"{a}_agt.svh").write_text(
            f"class {a}_agt extends uvm_agent;\n"
            f"  `uvm_component_utils({a}_agt)\n"
            f"  function new(string name, uvm_component parent); super.new(name, parent); endfunction\n"
            f"endclass : {a}_agt\n")
        pkg += [f'`include "agents/{a}_item.svh"', f'`include "agents/{a}_bfm.svh"', f'`include "agents/{a}_agt.svh"']
    (root / "tb" / "tb_classes.sv").write_text("\n".join(pkg) + "\n")
    (root / "tb" / "tb_top.sv").write_text("module tb_top;\n  int x;\n  initial x = x;\nendmodule : tb_top\n")


def run(binary, cwd, *args):
    env = dict(os.environ)
    env.pop("UVM_HOME", None)
    res = subprocess.run([binary, *args], cwd=cwd, capture_output=True, text=True, env=env)
    strip = lambda s: re.sub(r"\x1b\[[0-9;]*m", "", s)
    return res.returncode, strip(res.stdout), strip(res.stderr)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp, "proj")
        make_project(root)
        print("Discovery and naming inference")
        rc, out, err = run(binary, root, "--init")
        cfg_path = root / ".uvmcheck" / "config.py"
        check(rc == 0 and cfg_path.exists(), "config written", f"rc={rc}\n{out}\n{err}")
        cfg = cfg_path.read_text() if cfg_path.exists() else ""
        check('"tb/tb_classes.sv"' in cfg and '"tb/tb_top.sv"' in cfg, "top-level files are the sources", cfg)
        check(".svh" not in cfg.split("sources")[1].split("]")[0] if "sources" in cfg else False,
              "included headers are not sources", cfg)
        check('"UvmDriver"' in cfg and "_bfm" in cfg, "driver convention _bfm learnt", cfg)
        check('"UvmAgent"' in cfg and "_agt" in cfg, "agent convention _agt learnt", cfg)
        check('"UvmSequenceItem"' not in cfg, "default kept where the code follows it (_item)", cfg)
        base = root / ".uvmcheck" / "baseline.json"
        check(base.exists(), "baseline written", out)
        check(base.exists() and "UVM-NAM-101" not in base.read_text() and "UVM-NAM-106" not in base.read_text(),
              "house-style names are not baselined as naming findings")

        rc, out, err = run(binary, root)
        check("Total Violations: 0" in out, "plain run after init reports nothing new", out + err)

        rc, out, err = run(binary, root, "--init")
        check(rc == 2 and "already exists" in err, "refuses to overwrite an existing config", err)

        print("File list")
        root2 = Path(tmp, "proj2")
        make_project(root2)
        (root2 / "sim").mkdir()
        (root2 / "sim" / "files.f").write_text("+incdir+../tb\n../tb/tb_classes.sv\n../tb/tb_top.sv\n")
        rc, out, err = run(binary, root2, "--init")
        cfg = (root2 / ".uvmcheck" / "config.py").read_text() if rc == 0 else ""
        check(rc == 0 and '"sim/files.f"' in cfg, "readable file list used", f"rc={rc}\n{cfg}\n{err}")
        rc, out, err = run(binary, root2)
        check("Total Violations: 0" in out, "plain run with the file list is clean", out + err)

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll init checks passed.")


if __name__ == "__main__":
    main()
