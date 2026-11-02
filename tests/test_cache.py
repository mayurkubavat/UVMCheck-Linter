#!/usr/bin/env python3
"""Incremental result cache: a second run is served from .uvmcheck_cache/ with
the same JSON; editing an included file or an imported package invalidates the
units that depend on it; a rule option change invalidates; --no-cache bypasses;
a corrupt manifest falls back to a full run.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

failures = []

TOP = """`include "chip.svh"
import items::*;
module top;
endmodule
"""
CHIP = """module chip;
  logic [3:0] sel;
  always_comb begin
    casex (sel)
      4'b1xx0: ;
      default: ;
    endcase
  end
endmodule
"""
CHIP_FIXED = """module chip;
  logic [3:0] sel;
  always_comb begin
    case (sel)
      4'b1000: ;
      default: ;
    endcase
  end
endmodule
"""
PKG = """package items;
  class item;
    int x;
    function void f();
      if (x == x) $display("t");
    endfunction
  endclass
endpackage
"""
PKG_FIXED = """package items;
  class item;
    int x;
    function void f();
      if (x == 0) $display("t");
    endfunction
  endclass
endpackage
"""
OTHER = """package other_pkg;
  class other_c;
    logic [3:0] sel;
    function void f();
      casex (sel)
        4'b1xx0: ;
        default: ;
      endcase
    endfunction
  endclass
endpackage
"""


def check(cond, msg, detail=""):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)
        if detail:
            print("      " + detail[:2000].replace("\n", "\n      "))


def run(binary, cwd, *args):
    return subprocess.run([binary, *args], cwd=cwd, capture_output=True, text=True)


def violations(path):
    data = json.loads(Path(path).read_text())
    rows = []
    for v in data["violations"]:
        rows.append((v["ruleId"], v["file"], v["line"], v["column"], v["message"],
                     v.get("severity"), [(f.get("safety"), f.get("replacement")) for f in v.get("fixes", [])]))
    return sorted(rows)


def write_sources(d):
    (d / "top.sv").write_text(TOP)
    (d / "chip.svh").write_text(CHIP)
    (d / "pkg.sv").write_text(PKG)
    (d / "other.sv").write_text(OTHER)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default="build/uvmcheck")
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        write_sources(d)
        sources = ["top.sv", "other.sv", "pkg.sv"]

        first = run(binary, d, *sources, "--no-config", "--rule", "SV-COR-009", "--json", "a.json")
        check(first.returncode in (0, 1), "first run exits cleanly", first.stderr)
        check("cache: stored" in first.stdout, "first run stores the cache", first.stdout)
        check("served from cache" not in first.stdout, "first run is not a hit", first.stdout)
        check((d / ".uvmcheck_cache" / "manifest.json").is_file(), "manifest written")

        second = run(binary, d, *sources, "--no-config", "--rule", "SV-COR-009", "--json", "b.json")
        check("served from cache" in second.stdout, "second run is served from cache", second.stdout)
        check(violations(d / "a.json") == violations(d / "b.json"),
              "cached JSON matches the first run",
              f"first={violations(d / 'a.json')}\nsecond={violations(d / 'b.json')}")

        (d / "chip.svh").write_text(CHIP_FIXED)
        edited = run(binary, d, *sources, "--no-config", "--rule", "SV-COR-009", "--json", "c.json")
        check("cache miss: top.sv" in edited.stdout, "editing an include misses the includer", edited.stdout)
        check("cache hit: other.sv" in edited.stdout, "an unrelated unit stays a hit", edited.stdout)
        check("cache hit: pkg.sv" in edited.stdout, "a package that does not include the file stays a hit", edited.stdout)
        chip_left = [v for v in violations(d / "c.json") if v[1].endswith("chip.svh")]
        other_left = [v for v in violations(d / "c.json") if "other.sv" in v[1]]
        check(chip_left == [], "the included file's casex finding is gone", str(chip_left))
        check(other_left != [], "the unrelated file's finding is still reported", str(other_left))
        fresh = run(binary, d, *sources, "--no-config", "--no-cache", "--rule", "SV-COR-009", "--json", "d.json")
        check("served from cache" not in fresh.stdout, "--no-cache bypasses the cache", fresh.stdout)
        check(violations(d / "c.json") == violations(d / "d.json"),
              "partial cache matches a run with --no-cache",
              f"cached={violations(d / 'c.json')}\nfresh={violations(d / 'd.json')}")

        shutil.rmtree(d / ".uvmcheck_cache")
        write_sources(d)
        run(binary, d, *sources, "--no-config", "--rule", "SV-COR-008", "--json", "e.json")
        (d / "pkg.sv").write_text(PKG_FIXED)
        imp = run(binary, d, *sources, "--no-config", "--rule", "SV-COR-008", "--json", "f.json")
        check("cache miss: top.sv" in imp.stdout, "editing an imported package misses the importer", imp.stdout)
        check("cache miss: pkg.sv" in imp.stdout, "the edited package itself is a miss", imp.stdout)
        check("cache hit: other.sv" in imp.stdout, "a package that does not import it stays a hit", imp.stdout)
        pkg_left = [v for v in violations(d / "f.json") if v[1].endswith("pkg.sv") and v[0] == "SV-COR-008"]
        check(pkg_left == [], "the self-comparison finding is gone after the edit", str(pkg_left))

        (d / ".uvmcheck_cache" / "manifest.json").write_text("{ this is not json")
        bad = run(binary, d, *sources, "--no-config", "--rule", "SV-COR-008", "--json", "g.json")
        check(bad.returncode in (0, 1), "a corrupt cache does not crash", bad.stderr[-500:])
        check("served from cache" not in bad.stdout, "a corrupt cache is not served", bad.stdout)
        check("cache: stored" in bad.stdout, "a corrupt cache falls back to a full run", bad.stdout)

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "wide.sv").write_text("module wide;\n  int " + ("a" * 80) + ";\nendmodule\n")
        cfg = d / "config.py"
        cfg.write_text(
            "from uvmcheck.config import ProjectConfig, RuleConfig\n"
            "class Cfg(ProjectConfig):\n"
            "    class MaxLineLength(RuleConfig):\n"
            "        rule = 'SV-STY-016'\n"
            "        max_columns = 1000\n")
        wide = run(binary, d, "wide.sv", "--config", "config.py", "--rule", "SV-STY-016", "--json", "w.json")
        check(violations(d / "w.json") == [], "a high max_columns reports nothing", wide.stdout + wide.stderr)
        again = run(binary, d, "wide.sv", "--config", "config.py", "--rule", "SV-STY-016", "--json", "w2.json")
        check("served from cache" in again.stdout, "unchanged rule option is a cache hit", again.stdout)
        cfg.write_text(
            "from uvmcheck.config import ProjectConfig, RuleConfig\n"
            "class Cfg(ProjectConfig):\n"
            "    class MaxLineLength(RuleConfig):\n"
            "        rule = 'SV-STY-016'\n"
            "        max_columns = 40\n")
        tight = run(binary, d, "wide.sv", "--config", "config.py", "--rule", "SV-STY-016", "--json", "w3.json")
        check("served from cache" not in tight.stdout, "changing a rule option invalidates the cache", tight.stdout)
        ids = [v[0] for v in violations(d / "w3.json")]
        check("SV-STY-016" in ids, "the tightened option reports the long line", str(ids))

    # A rebuilt binary has the same version string but may have different rules or fix
    # text, so it must not reuse the old build's findings. A copy of the binary whose
    # modification time moves stands in for a rebuild.
    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "plain.sv").write_text("module plain; endmodule\n")
        exe = d / "uvmcheck_copy"
        shutil.copy2(binary, exe)
        run(str(exe), d, "plain.sv", "--no-config")
        same = run(str(exe), d, "plain.sv", "--no-config")
        check("served from cache" in same.stdout, "the same build is served from cache", same.stdout)
        st = exe.stat()
        os.utime(exe, ns=(st.st_atime_ns, st.st_mtime_ns + 10**9))
        rebuilt = run(str(exe), d, "plain.sv", "--no-config")
        check("served from cache" not in rebuilt.stdout, "a rebuilt binary misses the cache", rebuilt.stdout)

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        (d / "plain.sv").write_text("module plain; endmodule\n")
        bypass = run(binary, d, "plain.sv", "--no-config", "--no-cache")
        check(bypass.returncode in (0, 1), "--no-cache run exits cleanly", bypass.stderr)
        check(not (d / ".uvmcheck_cache").exists(), "--no-cache does not write a cache")

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        n = 40
        names = []
        for i in range(n):
            name = f"u{i}.sv"
            names.append(name)
            (d / name).write_text(
                f"module u{i};\n  logic [3:0] sel;\n  always_comb begin\n"
                "    casex (sel) 4'b1xx0: ; default: ; endcase\n  end\nendmodule\n")
        t0 = time.perf_counter()
        cold = run(binary, d, *names, "--no-config", "--rule", "SV-COR-009")
        cold_s = time.perf_counter() - t0
        t1 = time.perf_counter()
        warm = run(binary, d, *names, "--no-config", "--rule", "SV-COR-009")
        warm_s = time.perf_counter() - t1
        print(f"  [timing] cold {cold_s:.3f}s warm {warm_s:.3f}s files={n}")
        check(cold.returncode in (0, 1) and warm.returncode in (0, 1), "timing runs exit cleanly", warm.stderr[-400:])
        check("served from cache" in warm.stdout, "warm timing run is a cache hit", warm.stdout[-400:])

    if failures:
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nAll cache checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
