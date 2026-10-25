#!/usr/bin/env python3
"""
Checks for the UVM methodology guide (docs/methodology/):

- every rule the guide names (src/rules/Guidelines.cpp) is a registered rule;
- each chapter page lists, under each guideline, exactly the rules the linter maps to it;
- --guideline selects the guideline's rules and the JSON report carries a conformance
  section and per-finding guideline IDs;
- an unknown guideline is an error.
"""

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DOCS = REPO / "docs" / "methodology"
COMMON = REPO / "tests" / "common"

failures = []


def check(cond, msg):
    if not cond:
        failures.append(msg)


def run(binary, *args):
    return subprocess.run([binary, *args], capture_output=True, text=True, cwd=tempfile.gettempdir())


def guide(binary):
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "guide.json"
        res = run(binary, "--list-guidelines", "--json", str(out))
        check(res.returncode == 0, f"--list-guidelines --json failed: {res.stderr}")
        return json.loads(out.read_text())["guidelines"]


def registered_rules(binary):
    res = run(binary, "--list-rules")
    return {line.split()[0] for line in res.stdout.splitlines() if re.match(r"^[A-Z]+[A-Z0-9]*-[A-Z0-9]+-", line)}


def documented_rules():
    """{guideline id: [rule ids]} from the 'Enforced by' line under each '## MG-x.y' heading."""
    found = {}
    for page in sorted(DOCS.glob("MG-*.md")):
        current = None
        for line in page.read_text().splitlines():
            m = re.match(r"^## (MG-\d+\.\d+) ", line)
            if m:
                current = m.group(1)
                continue
            if current and line.startswith("Enforced by:"):
                found[current] = re.findall(r"`([A-Z0-9.\-]+)`", line)
                current = None
    return found


def test_mapping(binary):
    guidelines = guide(binary)
    check(len(guidelines) > 0, "guide is empty")
    known = registered_rules(binary)
    docs = documented_rules()
    readme = (DOCS / "README.md").read_text()
    for g in guidelines:
        for rule in g["rules"]:
            check(rule in known, f"{g['id']} names unknown rule {rule}")
        check(g["id"] in docs, f"{g['id']} has no 'Enforced by' line in docs/methodology")
        check(docs.get(g["id"], []) == g["rules"],
              f"{g['id']}: docs list {docs.get(g['id'])}, linter maps {g['rules']}")
        check(g["id"] in readme, f"{g['id']} missing from docs/methodology/README.md")
    for gid in docs:
        check(any(g["id"] == gid for g in guidelines), f"docs describe {gid}, which the linter does not know")


SAMPLE = """\
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_driver drv;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    drv = bus_driver::type_id::create("drv", this);
  endfunction
endclass
"""


def test_selection(binary):
    with tempfile.TemporaryDirectory() as tmp:
        src = Path(tmp) / "agent.sv"
        src.write_text(SAMPLE)
        report = Path(tmp) / "report.json"
        run(binary, "--guideline", "MG-4", "-I", str(COMMON), str(src), "--json", str(report))
        data = json.loads(report.read_text())
        ids = {v["ruleId"] for v in data["violations"]}
        check(ids == {"UVM-RUS-001"}, f"--guideline MG-4 reported {sorted(ids)}, expected only UVM-RUS-001")
        for v in data["violations"]:
            check(v.get("guidelines") == ["MG-4.1"], f"finding lists guidelines {v.get('guidelines')}")
        conf = {c["guideline"]: c for c in data.get("conformance", [])}
        check(conf.get("MG-4.1", {}).get("status") == "fail", f"MG-4.1 conformance: {conf.get('MG-4.1')}")
        check(conf.get("MG-4.4", {}).get("status") == "pass", f"MG-4.4 conformance: {conf.get('MG-4.4')}")
        check(all(c["guideline"].startswith("MG-4.") for c in conf.values()),
              f"conformance lists guidelines outside MG-4: {sorted(conf)}")

        # Without guideline selection the report has no conformance section unless asked for.
        run(binary, "-I", str(COMMON), str(src), "--json", str(report))
        check("conformance" not in json.loads(report.read_text()), "default run should not report conformance")
        run(binary, "--conformance", "-I", str(COMMON), str(src), "--json", str(report))
        check("conformance" in json.loads(report.read_text()), "--conformance should add a conformance section")

        res = run(binary, "--guideline", "MG-42", "-I", str(COMMON), str(src))
        check(res.returncode == 2 and "Unknown guideline" in res.stderr, "unknown guideline should be an error")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    test_mapping(binary)
    test_selection(binary)
    if failures:
        for f in failures:
            print(f"FAIL: {f}")
        return 1
    print("methodology guide checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
