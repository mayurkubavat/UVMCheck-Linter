#!/usr/bin/env python3
"""
Finding detail beyond file:line:column:

- `endColumn` covers the flagged identifier, or the whole statement where the
  rule reports one (SV-DUP-002 first cloned statement);
- related locations point at the other half of the finding: the first copy of
  a clone (SV-DUP-002) and the base-class property being shadowed (SV-OOP-004);
- the same detail reaches JSON, SARIF (region.endColumn, relatedLocations) and
  the console ("note:" lines).
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
failures = []


def check(cond, msg):
    print(f"  [{'PASS' if cond else 'FAIL'}] {msg}")
    if not cond:
        failures.append(msg)


def lint(binary, case, rule, tmp):
    src = REPO / "tests" / "rules" / case / "bad.sv"
    out_json = Path(tmp, rule + ".json")
    out_sarif = Path(tmp, rule + ".sarif")
    res = subprocess.run([binary, str(src), "-I", str(REPO / "tests" / "common"), "--rule", rule,
                          "--json", str(out_json), "--sarif", str(out_sarif)],
                         capture_output=True, text=True, cwd=tmp)
    return json.loads(out_json.read_text())["violations"], json.loads(out_sarif.read_text()), res.stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        print("SV-DUP-002 (clone fragments)")
        found, sarif, console = lint(binary, "methodology/SV-DUP-002", "SV-DUP-002", tmp)
        check(found, "finding reported")
        v = found[0]
        check(v.get("endColumn", 0) > v["column"] + 5, f"endColumn spans the statement ({v.get('endColumn')})")
        rel = v.get("related", [])
        check(len(rel) == 1 and rel[0]["line"] < v["line"] and "first copy" in rel[0]["message"],
              f"related location is the first copy ({rel})")
        result = sarif["runs"][0]["results"][0]
        region = result["locations"][0]["physicalLocation"]["region"]
        check(region.get("endColumn") == v.get("endColumn"), "SARIF region carries endColumn")
        srel = result.get("relatedLocations", [])
        check(len(srel) == 1 and srel[0]["physicalLocation"]["region"]["startLine"] == rel[0]["line"],
              "SARIF relatedLocations")
        check("note: " in console and "first copy" in console, "console prints a note line")

        print("SV-OOP-004 (variable shadowing)")
        found, _, _ = lint(binary, "oop/SV-OOP-004", "SV-OOP-004", tmp)
        prop = [f for f in found if "derived class" in f["message"]]
        check(prop, "property-shadowing finding reported")
        if prop:
            v = prop[0]
            rel = v.get("related", [])
            check(rel and "shadowed property" in rel[0]["message"] and rel[0]["line"] != v["line"],
                  f"related location is the base-class property ({rel})")
            check(v.get("endColumn", 0) > v["column"], "endColumn covers the property name")

        src = Path(tmp) / "nested.sv"
        src.write_text(
            "module nest;\n"
            "  int n;\n"
            "  task automatic shallow();\n"
            "    if (n == 0) n = 1;\n"
            "  endtask : shallow\n"
            "  task automatic deep();\n"
            "    if (n == 0) begin\n"
            "      if (n == 1) begin\n"
            "        if (n == 2) n = 3;\n"
            "      end\n"
            "    end\n"
            "  endtask : deep\n"
            "endmodule : nest\n")
        html = Path(tmp) / "nest.html"
        res = subprocess.run([binary, str(src), "--no-config", "--no-cache", "--html", str(html)],
                             capture_output=True, text=True, cwd=tmp)
        page = html.read_text() if html.exists() else ""
        deep = page.find("nest::deep")
        shallow = page.find("nest::shallow")
        check(res.returncode in (0, 1) and "Most complex routines" in page and deep != -1 and shallow != -1
              and deep < shallow, "HTML lists routines with deep ahead of shallow")

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll finding-detail checks passed.")


if __name__ == "__main__":
    main()
