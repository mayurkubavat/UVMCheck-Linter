#!/usr/bin/env python3
"""
The slang overlay (cpp/src/compat/, docs/reference/compat.md):

- every override is off by default, --list-overrides names them all, and none
  is still registered once the linked slang has the fix it stands in for;
- an unknown profile or override id is an error;
- a simulator file-list option (-i) is an error until filelist-dialect accepts it;
- vendor-systasks makes $fsdbDumpvars and friends known, while a name passed to
  one that does not resolve is still reported;
- auto-uvm compiles uvm_pkg.sv from $UVM_HOME when the sources use UVM and do
  not list it; uvm-library then keeps warnings inside the UVM library out of
  the report;
- error-taint downgrades findings inside a task or function that does not
  compile to Info, keeps the ones on the error's own line, and leaves other
  subroutines alone;
- opaque-ip folds the "unknown" errors for names an encrypted envelope defines
  into one Info finding, and compiles compat.ip_stubs as library code;
- global-clock (jasper profile) accepts $global_clock without global clocking;
- compat.system_tasks adds simulator tasks to vendor-systasks;
- crash-guard, on by default, turns a slang crash into one finding;
- lang-std, on by default, compiles as IEEE 1800-2023 unless --std chose a
  version, so 2023 constructs are not errors; -lang-std restores slang's 1800-2017;
- anchor-to-user-code, also on by default, reports an error slang
  raises inside a library template at the user's macro line, where the library
  policy would otherwise drop it;
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

REPO = Path(__file__).resolve().parents[2]
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
        rc, out, err = run(binary, root, "--list-addons")
        for oid in ("lang-std", "crash-guard", "filelist-dialect", "auto-uvm", "uvm-library", "opaque-ip",
                    "global-clock", "vendor-systasks", "slang-compat-vcs", "error-taint", "anchor-to-user-code"):
            check(rc == 0 and oid in out, f"--list-addons names {oid}", out + err)
        check(all(p in out for p in ("questa", "xcelium", "vcs", "jasper")), "--list-addons names the profiles", out)
        check("2020.3.1" in out, "--list-addons names the UVM API packs", out)
        rc, out2, _ = run(binary, root, "--list-overrides")
        check(rc == 0 and out2 == out, "--list-overrides is an alias of --list-addons")
        # An override whose fix the linked slang already has must be deleted, not kept around.
        check("[retired" not in out, "no override has outlived the slang bug it covers", out)
        (root / "empty.sv").write_text("module empty; endmodule\n")
        rc, _, err = run(binary, root, "--no-config", "empty.sv", "--compat-profile", "bogus")
        check(rc == 2 and "unknown compat profile 'bogus'" in err, "unknown profile is an error", err)
        rc, _, err = run(binary, root, "--no-config", "empty.sv", "--override", "nope")
        check(rc == 2 and "unknown override 'nope'" in err, "unknown override is an error", err)
        rc, rep, err = report(binary, root, "--no-config", "empty.sv")
        check(rep is not None and rep.get("compatOverrides") == ["lang-std", "crash-guard", "anchor-to-user-code"],
              "only lang-std, crash-guard and anchor-to-user-code are on by default", err or str(rep and rep.get("compatOverrides")))
        rc, rep, err = report(binary, root, "--no-config", "empty.sv", "--override", "-anchor-to-user-code")
        check(rep is not None and rep.get("compatOverrides") == ["lang-std", "crash-guard"], "-anchor-to-user-code turns it off",
              err or str(rep and rep.get("compatOverrides")))

        print("lang-std")
        (root / "ieee2023.sv").write_text(
            "package p2023;\n"
            "  class base;\n"
            "    rand int x; rand real r;\n"
            "    constraint c_r { r > 1.5; r < 9.0; }\n"
            "    constraint c_d { x dist {0 := 1, [1:9] :/ 3, default :/ 1}; }\n"
            "    virtual function :initial void f(); endfunction\n"
            "  endclass\n"
            "  class :final derived extends base;\n"
            "    constraint :extends c_r { r < 5.0; }\n"
            "    virtual function :extends :final void f(); endfunction\n"
            "  endclass\n"
            "  function automatic void g();\n"
            "    int a[] = '{1, 2, 3}; int b[];\n"
            "    string s = \"\"\"two\nlines\"\"\";\n"
            "    b = a.map(e) with (e * 2);\n"
            "    $display(\"%s %0d\", s, b.size());\n"
            "  endfunction\n"
            "endpackage\n")
        rc, rep, err = report(binary, root, "--no-config", "ieee2023.sv", "--rule", "SV-CMP-001")
        check(rep is not None and not compile_messages(rep), "1800-2023 constructs compile by default",
              err or str(compile_messages(rep)))
        rc, rep, err = report(binary, root, "--no-config", "ieee2023.sv", "--rule", "SV-CMP-001",
                              "--override", "-lang-std")
        check(any("1800-2017" in m for m in compile_messages(rep)), "-lang-std falls back to slang's 1800-2017",
              err or str(compile_messages(rep)))
        rc, rep, err = report(binary, root, "--no-config", "ieee2023.sv", "--rule", "SV-CMP-001",
                              "--std", "1800-2017")
        check(any("1800-2017" in m for m in compile_messages(rep)), "an explicit --std wins over lang-std",
              err or str(compile_messages(rep)))

        print("crash-guard")
        # Both still crash slang on the pinned commit. When one stops crashing (the
        # canary says so), its check below fails and the pattern can be retired.
        crashes = {
            "2066": "class c #(type T = enum {A, B});\n  T x;\nendclass\nmodule top; c #() o; endmodule\n",
            "2074": "interface ifc #(type T = virtual ifc);\n  T vif;\nendinterface\nmodule top; ifc i(); endmodule\n",
        }
        for issue, text in crashes.items():
            (root / f"crash{issue}.sv").write_text(text)
            rc, rep, err = report(binary, root, "--no-config", f"crash{issue}.sv")
            msgs = compile_messages(rep)
            check(rc == 1 and len(msgs) == 1 and f"#{issue}" in msgs[0] and "signal" in msgs[0],
                  f"slang #{issue} crash becomes one finding naming the issue", f"rc={rc} {err} {msgs}")
            rc, _, _ = run(binary, root, "--no-config", f"crash{issue}.sv", "--override", "-crash-guard")
            check(rc < 0 or rc > 128, f"-crash-guard lets slang #{issue} crash through", f"rc={rc}")
        rc, rep, err = report(binary, root, "--no-config", "ieee2023.sv")
        check(rep is not None and not any("crash-guard" in m for m in compile_messages(rep)),
              "crash-guard is transparent when slang does not crash", err)

        print("opaque-ip")
        ipdir = root / "ip"
        (ipdir / "stubs").mkdir(parents=True)
        (ipdir / "vip_enc.svp").write_text(
            "`pragma protect begin_protected\n"
            "`pragma protect encoding=(enctype=\"base64\", line_length=64, bytes=48)\n"
            "`pragma protect data_block\n"
            "aGVsbG8gd29ybGQgaGVsbG8gd29ybGQgaGVsbG8gd29ybGQgaGVsbG8gd29y\n"
            "`pragma protect end_protected\n")
        (ipdir / "tb.sv").write_text(
            "import vip_pkg::*;\nmodule tb;\n  logic clk;\n  vip_bfm bfm(.clk(clk));\n"
            "  initial begin vip_cfg c = new(); c.speed = 3; end\nendmodule\n")
        (ipdir / "stubs" / "vip_stubs.sv").write_text(
            "package vip_pkg;\n  class vip_cfg;\n    int speed;\n  endclass\nendpackage\n")
        files = ["vip_enc.svp", "tb.sv"]
        rc, rep, err = report(binary, ipdir, "--no-config", *files)
        check(any("unknown package 'vip_pkg'" in m for m in compile_messages(rep)),
              "without opaque-ip, names defined in the envelope are errors", str(compile_messages(rep)))
        rc, rep, err = report(binary, ipdir, "--no-config", *files, "--override", "opaque-ip")
        errs = [v for v in (rep or {}).get("violations", []) if v["ruleId"] == "SV-CMP-001"]
        folded = [v for v in (rep or {}).get("violations", []) if "[opaque-ip]" in v["message"]]
        check(not errs and len(folded) == 1 and folded[0]["severity"].lower() == "info" and
              "vip_pkg" in folded[0]["message"] and "vip_bfm" in folded[0]["message"],
              "opaque-ip folds them into one Info finding naming them", str(errs) + str(folded))
        check(not any(v["file"].endswith("vip_enc.svp") for v in (rep or {}).get("violations", [])),
              "the envelope's own warnings are dropped")
        (ipdir / "cfg.json").write_text(json.dumps({"compat": {"overrides": ["opaque-ip"], "ip_stubs": ["stubs"]}}))
        rc, rep, err = report(binary, ipdir, "--config", "cfg.json", *files)
        folded = [v for v in (rep or {}).get("violations", []) if "[opaque-ip]" in v["message"]]
        check(len(folded) == 1 and "vip_bfm" in folded[0]["message"] and "vip_pkg" not in folded[0]["message"] and
              not any("stubs" in v["file"] for v in rep.get("violations", [])),
              "compat.ip_stubs supplies vip_pkg as library code; only vip_bfm stays unknown", str(folded) + err)

        print("global-clock")
        (root / "gclk.sv").write_text("module gclk(input logic a);\n  assert property (@($global_clock) a);\n"
                                      "  assert property (@($global_clock) !a);\nendmodule\n")
        rc, rep, err = report(binary, root, "--no-config", "gclk.sv")
        check(sum("no global clocking" in m for m in compile_messages(rep)) == 2,
              "without global-clock, $global_clock with no global clocking is an error", str(compile_messages(rep)))
        rc, rep, err = report(binary, root, "--no-config", "gclk.sv", "--compat-profile", "jasper")
        folded = [v for v in (rep or {}).get("violations", []) if "[global-clock]" in v["message"]]
        check(rep is not None and "global-clock" in rep.get("compatOverrides", []) and
              not [m for m in compile_messages(rep) if "no global clocking" in m and "[global-clock]" not in m] and
              len(folded) == 1 and folded[0]["message"].startswith("2 use"),
              "the jasper profile folds them into one Info finding", str(compile_messages(rep)))

        print("system_tasks")
        (root / "systask.sv").write_text("module systask; initial $my_dump(systask); endmodule\n")
        (root / "systask.json").write_text(json.dumps({"compat": {"overrides": ["vendor-systasks"],
                                                                   "system_tasks": ["$my_dump"]}}))
        rc, rep, err = report(binary, root, "--no-config", "systask.sv", "--override", "vendor-systasks")
        check(any("my_dump" in m for m in compile_messages(rep)), "an unlisted task is unknown", str(compile_messages(rep)))
        rc, rep, err = report(binary, root, "--config", "systask.json", "systask.sv")
        check(rep is not None and not compile_messages(rep), "compat.system_tasks makes it known",
              err + str(compile_messages(rep)))

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
        check(rep is not None and rep.get("compatOverrides") == ["lang-std", "crash-guard", "vendor-systasks", "anchor-to-user-code"],
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

        print("error-taint")
        (root / "taint.sv").write_text(
            "module m;\n"
            "  function void broken();\n"
            "    #1;\n"
            "  endfunction\n"
            "  function void clean();\n"
            "  endfunction\n"
            "endmodule : m\n")

        def end_labels(rep):
            return {v["line"]: v for v in (rep or {}).get("violations", []) if v["ruleId"] == "SV-STY-104"}

        rc, rep, err = report(binary, root, "--no-config", "taint.sv")
        before = end_labels(rep)
        check(before.get(4, {}).get("severity") == "WARNING" and before.get(6, {}).get("severity") == "WARNING",
              "without the override both missing end labels are warnings", str(before))
        rc, rep, err = report(binary, root, "--no-config", "taint.sv", "--override", "error-taint")
        after = end_labels(rep)
        check(after.get(4, {}).get("severity") == "INFO" and "error-taint" in after.get(4, {}).get("message", ""),
              "a finding in a subroutine that does not compile becomes Info", str(after))
        check(after.get(6, {}).get("severity") == "WARNING", "a subroutine that compiles is left alone", str(after))
        check(any(v["ruleId"] == "SV-CMP-001" and v["line"] == 3 and v["severity"] == "ERROR"
                  for v in rep["violations"]), "the compile error itself stays an error", str(rep["violations"]))

        print("anchor-to-user-code")
        lib = root / "lib"
        lib.mkdir()
        (lib / "registry.svh").write_text(
            "class registry #(type T = int);\n"
            "  static function T create();\n"
            "    T obj = new(\"obj\");\n"
            "    return obj;\n"
            "  endfunction\n"
            "endclass\n"
            "`define my_utils(T) typedef registry#(T) type_id;\n")
        (root / "user.sv").write_text(
            '`include "lib/registry.svh"\n'
            "class comp;\n"
            "  function new(string name, int parent);\n"
            "  endfunction\n"
            "  `my_utils(comp)\n"
            "endclass\n")
        args = ("--no-config", "user.sv", "--library", "lib/**", "--rule", "SV-CMP-001")
        rc, rep, err = report(binary, root, *args, "--override", "-anchor-to-user-code")
        check(rep is not None and rep["compileErrors"] > 0 and not compile_messages(rep),
              "without the override the template error is dropped with the library", err or str(rep))
        rc, rep, err = report(binary, root, *args)
        errs = [v for v in (rep or {}).get("violations", []) if v["ruleId"] == "SV-CMP-001"]
        check(len(errs) == 1 and errs[0]["file"].endswith("user.sv") and errs[0]["line"] == 5,
              "by default the error is reported once, at the user's macro line", str(errs))
        check(bool(errs) and any(r["file"].endswith("registry.svh") and r["line"] == 3
                                 for r in errs[0].get("related", [])),
              "the library line is kept as a note", str(errs))

        print("Config")
        (tb / "cfg.json").write_text(json.dumps({
            "compat": {"profile": "questa", "uvm_home": str(root / "uvm-core")}}))
        rc, rep, err = report(binary, tb, "--config", "cfg.json", "test.sv")
        check(rep is not None and not compile_messages(rep) and
              rep.get("compatOverrides") == ["lang-std", "crash-guard", "filelist-dialect", "auto-uvm", "uvm-library", "vendor-systasks",
                                             "anchor-to-user-code"],
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
