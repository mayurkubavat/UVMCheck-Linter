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

REPO = Path(__file__).resolve().parents[2]
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

        print("Message regex")
        scoped = root / "scoped"
        scoped.mkdir()
        (scoped / "user_tb.sv").write_text(
            "module user_tb;\n  int a, b;\n  initial begin\n"
            "    if (a == a) $display(\"one\"); // uvmcheck:waive SV-COR-008 \"expression 'a'\"\n"
            "    // uvmcheck: waive SV-COR-008 \"expression 'b'\"\n"
            "    if (b == b) $display(\"two\");\n"
            "  end\nendmodule\n")
        (scoped / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, Waiver\n"
            "class Project(ProjectConfig):\n"
            "    waivers = [Waiver('SV-COR-008', reason='only a', message=\"expression 'a'\")]\n")
        # The inline comments already waive both lines; drop them so the config regex is what decides.
        (scoped / "user_tb.sv").write_text(
            "module user_tb;\n  int a, b;\n  initial begin\n"
            "    if (a == a) $display(\"one\");\n"
            "    if (b == b) $display(\"two\");\n"
            "  end\nendmodule\n")
        rc, out = run(binary, scoped, "--config", "config.py", "--rule", "SV-COR-008", "--json", "out.json")
        import json
        found = json.loads((scoped / "out.json").read_text())["violations"]
        lines = sorted(v["line"] for v in found if v["ruleId"] == "SV-COR-008")
        check(lines == [5], f"message regex waives only the 'a' finding, got {lines}", out)

        (scoped / "user_tb.sv").write_text(
            "module user_tb;\n  int a, b;\n  initial begin\n"
            "    if (a == a) $display(\"one\"); // uvmcheck:waive SV-COR-008 \"expression 'a'\"\n"
            "    // uvmcheck: waive SV-COR-008 \"no-such-text\"\n"
            "    if (b == b) $display(\"two\");\n"
            "  end\nendmodule\n")
        rc, out = run(binary, scoped, "--no-config", "--rule", "SV-COR-008", "--json", "out.json")
        found = json.loads((scoped / "out.json").read_text())["violations"]
        lines = sorted(v["line"] for v in found if v["ruleId"] == "SV-COR-008")
        check(lines == [6], f"inline message regex waives only a matching diagnostic, got {lines}", out)

        (scoped / "user_tb.sv").write_text(
            "module user_tb;\n  int a, b;\n  initial begin\n"
            "    if (a == a) $display(\"one\");\n"
            "    // uvmcheck: waive SV-COR-008 \"expression 'b'\"\n"
            "    if (b == b) $display(\"two\");\n"
            "  end\nendmodule\n")
        rc, out = run(binary, scoped, "--no-config", "--rule", "SV-COR-008", "--json", "out.json")
        found = json.loads((scoped / "out.json").read_text())["violations"]
        lines = sorted(v["line"] for v in found if v["ruleId"] == "SV-COR-008")
        check(lines == [4], f"'uvmcheck: waive' with a message regex waives the next line, got {lines}", out)

        (scoped / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, Waiver\n"
            "class Project(ProjectConfig):\n"
            "    waivers = [Waiver('SV-COR-009', reason='nothing casex', message='casex')]\n")
        rc, out = run(binary, scoped, "--config", "config.py", "--report-unused-waivers", "--rule", "SV-COR-008")
        check("UVM-WV-001" in out and "matched no diagnostic" in out,
              "--report-unused-waivers warns about a waiver that matched nothing", out)
        rc, quiet = run(binary, scoped, "--config", "config.py", "--rule", "SV-COR-008")
        check("UVM-WV-001" not in quiet, "unused waivers stay quiet without the flag", quiet)

        (scoped / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, Waiver\n"
            "class Project(ProjectConfig):\n"
            "    waivers = [Waiver('SV-COR-008', reason='bad', message='(')]\n")
        rc, out = run(binary, scoped, "--config", "config.py")
        check(rc == 2 and "config.py" in out and "Waiver(SV-COR-008)" in out and "invalid message regex" in out,
              "invalid message regex names the config file and the waiver", out)

        (scoped / "waivers.json").write_text(
            '{"waivers":[{"rule":"SV-COR-008","reason":"bad","message":"("}]}')
        rc, out = run(binary, scoped, "--no-config", "--waivers", "waivers.json")
        check(rc == 2 and "waivers.json" in out and "invalid message regex" in out,
              "invalid message regex in a waiver file names that file", out)

        print("Severity re-grade")
        (scoped / "user_tb.sv").write_text(
            "module user_tb;\n  int a, b;\n  initial begin\n"
            "    if (a == a) $display(\"one\");\n"
            "    if (b == b) $display(\"two\");\n"
            "  end\nendmodule\n")
        (scoped / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, Waiver, Severity\n"
            "class Project(ProjectConfig):\n"
            "    waivers = [Waiver('SV-COR-008', reason='tracked', message=\"expression 'b'\", severity=Severity.Info)]\n")
        rc, out = run(binary, scoped, "--config", "config.py", "--rule", "SV-COR-008", "--json", "out.json")
        found = [v for v in json.loads((scoped / "out.json").read_text())["violations"] if v["ruleId"] == "SV-COR-008"]
        sev = {v["line"]: v["severity"] for v in found}
        check(sev == {4: "ERROR", 5: "INFO"}, f"severity waiver keeps the finding at INFO, got {sev}", out)
        check(any("[re-graded by" in v["message"] for v in found), "re-graded finding says so")
        check("re-graded by waivers" in out, "run summary counts re-graded findings", out)

        (scoped / "waivers.json").write_text(
            '{"waivers":[{"rule":"SV-COR-008","reason":"r","severity":"warning"}]}')
        rc, out = run(binary, scoped, "--no-config", "--waivers", "waivers.json", "--rule", "SV-COR-008",
                      "--json", "out.json")
        found = [v for v in json.loads((scoped / "out.json").read_text())["violations"] if v["ruleId"] == "SV-COR-008"]
        check(len(found) == 2 and all(v["severity"] == "WARNING" for v in found),
              "waiver file severity re-grades", out)

        (scoped / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig, Waiver\n"
            "class Project(ProjectConfig):\n"
            "    waivers = [Waiver('SV-COR-008', reason='x', severity='fatal')]\n")
        rc, out = run(binary, scoped, "--config", "config.py")
        check(rc == 2 and "unknown severity" in out, "an unknown waiver severity is a config error", out)

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll waiver hygiene checks passed.")


if __name__ == "__main__":
    main()
