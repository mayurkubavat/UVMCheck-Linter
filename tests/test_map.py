#!/usr/bin/env python3
"""
--map: the testbench map built from tests/map/map_tb.sv (test > env > agent
> driver/monitor, scoreboard) has the right tree, the TLM connection split
into source and target, the unconnected port, config_db flows with the
unmatched get and the unread set, and findings attached to components; the
HTML form renders the same tree.
"""

import argparse
import json
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
            print("      " + str(detail)[:2000])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())
    with tempfile.TemporaryDirectory() as tmp:
        base = [binary, "--no-config", str(REPO / "tests/map/map_tb.sv"), "-I", str(REPO / "tests/common")]
        subprocess.run(base + ["--map", "map.json"], cwd=tmp, capture_output=True)
        subprocess.run(base + ["--map", "map.html"], cwd=tmp, capture_output=True)
        data = json.loads(Path(tmp, "map.json").read_text())
        comps = {c["name"]: c for c in data["components"]}
        kids = lambda n: [(k["instance"], k["class"]) for k in comps[n]["children"]]

        check(data["roots"][0] == "my_test", "the test is the first root", data["roots"])
        check(kids("my_test") == [("env", "my_env")] and kids("my_env") == [("agt", "my_agent"), ("scb", "my_scb")]
              and kids("my_agent") == [("drv", "my_drv"), ("mon", "my_mon")], "component tree")
        check(data["connections"] == [{"in": "my_env", "from": "agt.mon.ap", "to": "scb.imp"}],
              "connection split into source and target", data["connections"])
        ports = {p["name"]: p["connected"] for p in comps["my_mon"]["ports"]}
        check(ports == {"ap": True, "dbg_ap": False}, "unconnected port flagged", ports)
        flows = {(c["op"], c["field"]): c["matched"] for c in data["config_db"]}
        check(flows.get(("get", "timeout")) is False and flows.get(("set", "unused_knob")) is False
              and flows.get(("set", "depth")) is True, "config_db flows", flows)
        check(any(f["rule"] == "UVM-PHS-001" for f in comps["my_env"]["findings"]),
              "findings attached to the component they are in")

        page = Path(tmp, "map.html").read_text()
        check("<title>Testbench map</title>" in page and "agt.mon.ap" in page and "never read" in page,
              "HTML page rendered")

    if failures:
        print(f"\n{len(failures)} failure(s)")
        sys.exit(1)
    print("\nAll testbench map checks passed.")


if __name__ == "__main__":
    main()
