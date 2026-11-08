#!/usr/bin/env python3
"""
--fix and --fix-dry-run apply only safe fixes. casex/case-equality rewrites
change X/Z matching, so they are skipped until --unsafe-fixes. A display-only
suggestion (at_least) is never written. The dry run prints a unified diff and
changes nothing; the diff applies cleanly and gives the same text as --fix;
fixes in an included file land in that file; a second --fix finds nothing to do.
One --fix repeats passes until the text stops changing, so the final report is of the
fixed text. A fix that would drop a macro call is shown but not applied; a file with
compile errors is never rewritten; fixes that would undo each other do not ping-pong.
"""

import argparse
import json
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
HELPER = """`ifndef HELPERS_SVH
`define HELPERS_SVH
module fix_helper;
    logic b, z;
    always_comb begin
        if (b != 1'bz) z = 0;
        else z = 1;
    end
endmodule : fix_helper
`endif
"""


UVM_TOP = """`include "uvm_mock.svh"
class fx_item extends uvm_sequence_item;
    rand int a;
    function new(string name = "fx_item");
        super.new(name);
    endfunction
endclass : fx_item

class fx_env extends uvm_component;
    `uvm_component_utils(fx_env)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void build_phase(uvm_phase phase);
        string name = "x";
        $display("building %s", get_full_name());
        if (phase == null) $display("no phase");
        else $display("ok");
        $display(name);
    endfunction
endclass : fx_env
"""

MULTIPASS = """class mp_box;
    rand int len;
    constraint len_c {
        len > 0;
        len < 256;
        len > 0;
    }
endclass : mp_box
"""

MACRO_TOP = """`define MSB 0
module mc_top;
    logic [7:0] d;
    logic y;
    assign y = d[1+`MSB:7];
endmodule : mc_top
"""

BROKEN_TOP = """module br_top;
    logic a, y;
    always_comb begin
        if (a == 1'bx) y = 0;
        else y = 1;
    end
endmodule : br_top

module br_other;
    logic z;
    assign z = undeclared_signal;
endmodule : br_other
"""

OSCILLATE_TOP = """interface osc_if;
    logic valid;
endinterface : osc_if

class osc_drv;
    virtual osc_if vif;
    function void reset_bus();
        vif.valid = 0;
    endfunction : reset_bus
endclass : osc_drv
"""

STYLE_TOP = """`define uvm_note(M) begin $display(M); end
module st_top;
    logic [63:0] a;
    logic [7:0] b;
    initial begin
        a = 'hFF;
        b = 'bx;
        if(a == 0) b = 1;
        b = 2; a = 3; b = 4;
        `uvm_note("hi");
    end
endmodule : st_top
"""

STYLE_HELPER = """class helper_c;
    int v;
endclass
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
        check("0 fixes would be applied, 3 unsafe fixes skipped (use --unsafe-fixes)" in res.stdout,
              "dry run skips unsafe fixes", res.stdout)
        check("casez" not in extract_diff(res.stdout), "dry run does not preview unsafe rewrites", res.stdout)

        res = run(binary, real, "--fix")
        check("0 fixes applied, 3 unsafe fixes skipped (use --unsafe-fixes)" in res.stdout,
              "--fix leaves unsafe fixes unapplied", res.stdout)
        check((real / "top.sv").read_text() == TOP and (real / "helpers.svh").read_text() == HELPER,
              "--fix does not rewrite casex or case equality")

        subprocess.run([binary, "top.sv", "--no-config", "--json", "out.json",
                        "--sarif", "out.sarif", "--html", "out.html"],
                       cwd=real, capture_output=True, text=True)
        report = json.loads((real / "out.json").read_text())
        casex = [v for v in report["violations"] if v["ruleId"] == "SV-COR-009"]
        check(casex and casex[0]["fixes"][0]["safety"] == "unsafe" and casex[0]["fixes"][0]["replacement"] == "casez",
              "JSON carries the unsafe fix", casex)
        sarif = json.loads((real / "out.sarif").read_text())
        sfixes = [f for r in sarif["runs"][0]["results"] if r["ruleId"] == "SV-COR-009" for f in r.get("fixes", [])]
        check(sfixes and sfixes[0]["properties"]["safety"] == "unsafe", "SARIF fix property is unsafe", sfixes)
        page = (real / "out.html").read_text()
        check('data-safety="unsafe"' in page, "HTML report shows the unsafe fix", page)

        res = run(binary, dry, "--fix-dry-run", "--unsafe-fixes")
        check((dry / "top.sv").read_text() == TOP, "unsafe dry run still changes no file")
        check("Would apply 3 fix-it(s) in 2 file(s)" in res.stdout, "unsafe dry run counts the fixes", res.stdout)
        diff = extract_diff(res.stdout)
        check("-        casex (sel)" in diff and "+        casez (sel)" in diff, "diff shows the rewrite", diff)

        res = run(binary, real, "--fix", "--unsafe-fixes")
        check("Applied 3 fix-it(s) in 2 file(s)" in res.stdout, "--unsafe-fixes applies the fixes", res.stdout)
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

        only = Path(tmp, "only")
        only.mkdir()
        setup(only)
        res = run(binary, only, "--fix", "--unsafe-fixes", "--fix-rule", "SV-COR-009")
        top = (only / "top.sv").read_text()
        check("casez (sel)" in top and "a == 1'bx" in top and (only / "helpers.svh").read_text() == HELPER,
              "--fix-rule limits the fixes to that rule", top)

        cov = Path(tmp, "cov")
        cov.mkdir()
        (cov / "top.sv").write_text(
            "module cov_m;\n  logic clk, x;\n  covergroup cg @(posedge clk);\n"
            "    cp: coverpoint x { option.at_least = 0; }\n  endgroup\nendmodule\n")
        res = subprocess.run([binary, "top.sv", "--no-config", "--rule", "SV-COV-011",
                              "--fix", "--unsafe-fixes", "--json", "out.json"],
                             cwd=cov, capture_output=True, text=True)
        text = (cov / "top.sv").read_text()
        check("at_least = 0" in text, "display-only fix is not applied", text)
        cov_report = json.loads((cov / "out.json").read_text())
        shown = [f for v in cov_report["violations"] if v["ruleId"] == "SV-COV-011" for f in v.get("fixes", [])]
        check(shown and shown[0]["safety"] == "display-only", "JSON reports the display-only fix", cov_report)

        # An out-of-block method is labelled with its own name, not cls::name.
        ob = Path(tmp, "outofblock")
        ob.mkdir()
        (ob / "top.sv").write_text("class ob_c;\n    extern function void go();\nendclass : ob_c\n"
                                   "function void ob_c::go();\nendfunction\n")
        run(binary, ob, "--fix", "--fix-rule", "SV-STY-005")
        text = (ob / "top.sv").read_text()
        check("endfunction : go\n" in text, "end label fix uses the method name", text)

        # UVM fixes: factory registration, super.<phase>(), $display to `uvm_info. All unsafe.
        uv = Path(tmp, "uvm")
        uv.mkdir()
        common = str(Path(__file__).resolve().parent.parent / "testdata" / "common")
        (uv / "top.sv").write_text(UVM_TOP)
        uvm_rules = "UVM-FAC-003,UVM-PHS-001,UVM-LOG-002"
        res = run(binary, uv, "-I", common, "--fix", "--fix-rule", uvm_rules)
        check((uv / "top.sv").read_text() == UVM_TOP, "UVM fixes are unsafe: plain --fix skips them", res.stdout)
        res = run(binary, uv, "-I", common, "--fix", "--unsafe-fixes", "--fix-rule", uvm_rules)
        text = (uv / "top.sv").read_text()
        check("class fx_item extends uvm_sequence_item;\n    `uvm_object_utils(fx_item)\n" in text,
              "missing `uvm_object_utils is added after the class header", text)
        check('string name = "x";\n        super.build_phase(phase);\n' in text,
              "super.build_phase(phase) becomes the first statement, after the declarations", text)
        check('`uvm_info(get_type_name(), $sformatf("building %s", get_full_name()), UVM_LOW)\n' in text and
              'else `uvm_info(get_type_name(), $sformatf("ok"), UVM_LOW)\n' in text,
              "$display statements become `uvm_info, also as an if/else branch", text)
        check("$display(name)" in text, "a $display without a format string is left alone", text)
        res = run(binary, uv, "-I", common, "--rule", "UVM-PHS-001")
        check("Build succeeded: 0 errors" in res.stdout and "UVM-PHS-001" not in res.stdout,
              "fixed file compiles and the finding is gone", res.stdout)

        # One --fix keeps going until nothing changes: deleting the duplicate constraint
        # leaves an indented blank line, whose whitespace fix only shows up on pass two.
        mp = Path(tmp, "multipass")
        mp.mkdir()
        (mp / "top.sv").write_text(MULTIPASS)
        res = run(binary, mp, "--fix", "--fix-rule", "SV-RND-011,SV-STY-015,SV-STY-102")
        applied = [l for l in res.stdout.splitlines() if l.startswith("[uvmcheck] Applied ")]
        check(len(applied) == 2, "a fix that uncovers another finding gets a second pass", res.stdout)
        res = run(binary, mp, "--fix", "--fix-rule", "SV-RND-011,SV-STY-015,SV-STY-102")
        check("fix-it" not in res.stdout, "after one --fix run there is nothing left to fix", res.stdout)
        res = run(binary, mp, "--fix-rule", "SV-RND-011", "--rule", "SV-RND-011")
        check("SV-RND-011" not in res.stdout, "the final report is of the fixed text", res.stdout)

        # A rewrite of written text around a macro call, built from the expansion, would
        # inline the macro (`MSB disappears). It is shown, never applied.
        mc = Path(tmp, "macro")
        mc.mkdir()
        (mc / "top.sv").write_text(MACRO_TOP)
        report = mc / "r.json"
        res = run(binary, mc, "--fix", "--unsafe-fixes", "--json", str(report))
        check((mc / "top.sv").read_text() == MACRO_TOP, "a fix that would drop a macro call is not applied",
              res.stdout)
        found = [f for v in json.loads(report.read_text())["violations"] if v["ruleId"] == "SV-LANG-004"
                 for f in v.get("fixes", [])]
        check(found and found[0]["safety"] == "display-only" and "macro call" in found[0]["description"],
              "the report shows it as display-only, saying why", found)

        # A file slang could not compile is never rewritten.
        br = Path(tmp, "broken")
        br.mkdir()
        (br / "top.sv").write_text(BROKEN_TOP)
        res = run(binary, br, "--fix", "--unsafe-fixes")
        check((br / "top.sv").read_text() == BROKEN_TOP and "in files with compile errors not applied" in res.stdout,
              "fixes in a file with compile errors are not applied", res.stdout)

        # Two rules that undo each other's fix (SV-INT-001 '=' -> '<=', SV-LANG-005 '<=' in a
        # function -> '='): inside a function the interface-drive fix is only shown.
        osc = Path(tmp, "oscillate")
        osc.mkdir()
        (osc / "top.sv").write_text(OSCILLATE_TOP)
        res = run(binary, osc, "--fix", "--unsafe-fixes")
        check("undo each other" not in res.stderr and "vif.valid = 0;" in (osc / "top.sv").read_text(),
              "an interface drive in a function is not flipped back and forth", res.stdout + res.stderr)

        # Style fixes: spacing, one statement per line, macro semicolons, literal widths,
        # end labels, include guards. A plain --fix applies all but the include guard.
        sf = Path(tmp, "style")
        sf.mkdir()
        (sf / "top.sv").write_text(STYLE_TOP)
        (sf / "helper.svh").write_text(STYLE_HELPER)
        style_rules = "SV-STY-002,SV-STY-010,SV-STY-011,SV-STY-012,SV-STY-013,SV-STY-104"
        res = subprocess.run([binary, "top.sv", "helper.svh", "--no-config", "--preset", "all", "--fix",
                              "--fix-rule", style_rules], cwd=sf, capture_output=True, text=True)
        text = (sf / "top.sv").read_text()
        check("        if (a == 0) b = 1;\n" in text, "a space is put between 'if' and '('", text)
        check("        b = 2;\n        a = 3;\n        b = 4;\n" in text,
              "statements sharing a line each get their own line, at the same indent", text)
        check('`uvm_note("hi")\n' in text, "the semicolon after a `uvm_ macro call is removed", text)
        check("a = 32'hFF;" in text and "b = 'bx;" in text,
              "an unsized based literal gets its width, but not one with x/z digits", text)
        check("`ifndef" not in (sf / "helper.svh").read_text(), "the include guard is unsafe: plain --fix skips it",
              res.stdout)
        subprocess.run([binary, "top.sv", "helper.svh", "--no-config", "--preset", "all", "--fix", "--unsafe-fixes",
                        "--fix-rule", style_rules], cwd=sf, capture_output=True, text=True)
        helper = (sf / "helper.svh").read_text()
        check(helper.startswith("`ifndef HELPER_SVH\n`define HELPER_SVH\n") and
              helper.rstrip().endswith("`endif // HELPER_SVH") and "endclass : helper_c" in helper,
              "--unsafe-fixes wraps the header in a guard, and the end label fix also lands", helper)

    if failures:
        print(f"{len(failures)} check(s) failed")
        return 1
    print("All fix checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
