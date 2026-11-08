#!/usr/bin/env python3
"""Measures which non-standard SystemVerilog each tool accepts.

testdata/vendor/*.sv are probes: each is legal SystemVerilog except for one
construct IEEE 1800 forbids and some simulators accept anyway (a name used
before its declaration, a hierarchical name in a constant, a bare `#W`
parameter value, ...). Its first line names the slang option that accepts it
(`// slang: --allow-...`, or `none`); its top module is `probe`.

`measure` compiles every probe with one tool and writes what it accepted to
data/vendor/<tool>.json, with the tool's version and the date: a measured
table, not one copied from documentation. `table` prints all measured tables
side by side (Markdown). `slang-flags` prints the slang options that make
uvmcheck accept what a measured tool accepts, for use as

    uvmcheck $(tools/quality/vendor_table.py slang-flags data/vendor/vcs.json) -f files.f

  tools/quality/vendor_table.py measure verilator|iverilog|vcs|xcelium|questa|uvmcheck|dv-build [--out DIR]
  tools/quality/vendor_table.py table [DIR]
  tools/quality/vendor_table.py slang-flags data/vendor/<tool>.json

The commercial simulators are measured on a machine that has them:
`measure vcs` runs `vcs`, `measure xcelium` runs `xrun`, `measure questa`
runs `qrun` (each must be on PATH). Probes a tool rejects for a reason other
than the construct show in its table with the tool's message.
"""

import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
PROBES = REPO / "testdata" / "vendor"
OUT = REPO / "data" / "vendor"
UVMCHECK = os.environ.get("UVMCHECK", str(REPO / "build" / "uvmcheck"))
DVBUILD = os.environ.get("DV_BUILD", str(REPO / "target" / "release" / "dv-build"))

# tool -> (version command, compile command for one file; run in a scratch dir)
TOOLS = {
    "verilator": (["verilator", "--version"], lambda f: ["verilator", "--lint-only", "-Wno-fatal", "--timing", "--top-module", "probe", f]),
    "iverilog": (["iverilog", "-V"], lambda f: ["iverilog", "-g2012", "-s", "probe", "-o", "a.out", f]),
    "vcs": (["vcs", "-ID"], lambda f: ["vcs", "-full64", "-sverilog", "-top", "probe", "-o", "simv", f]),
    "xcelium": (["xrun", "-version"], lambda f: ["xrun", "-sv", "-elaborate", "-top", "probe", f]),
    "questa": (["qrun", "-version"], lambda f: ["qrun", "-sv", "-optimize", "-top", "probe", f]),
    "uvmcheck": ([UVMCHECK, "--version"], lambda f: [UVMCHECK, "--no-config", "--no-cache", "--rule", "SV-CMP-001", f]),
    "dv-build": ([DVBUILD, "--help"], lambda f: [DVBUILD, f]),
}


def probes():
    out = []
    for f in sorted(PROBES.glob("*.sv")):
        lines = f.read_text().splitlines()
        flag = re.match(r"//\s*slang:\s*(\S+)", lines[0])
        what = lines[1].lstrip("/ ").strip() if len(lines) > 1 else ""
        out.append((f.stem, f, flag.group(1) if flag else "none", what))
    return out


def accepted(tool, res):
    text = res.stdout + res.stderr
    if tool == "uvmcheck":
        return "SV-CMP-001 (" not in text
    if tool == "dv-build":
        return res.returncode == 0
    if tool == "verilator":
        return res.returncode == 0 and "%Error" not in text
    return res.returncode == 0


def first_error(text):
    for line in text.splitlines():
        if re.search(r"error|Error|ERROR", line):
            return re.sub(r"\x1b\[[0-9;]*m", "", line).strip()[:200]
    return ""


def measure(tool, out_dir):
    ver_cmd, compile_cmd = TOOLS[tool]
    try:
        v = subprocess.run(ver_cmd, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        print(f"{tool}: cannot run {ver_cmd[0]}: {e}", file=sys.stderr)
        return 2
    version = (v.stdout or v.stderr).strip().splitlines()[0] if (v.stdout or v.stderr).strip() else "unknown"
    if tool in ("dv-build", "uvmcheck"):
        # Ours: the commit it was built from says more than a version number.
        g = subprocess.run(["git", "-C", str(REPO), "describe", "--always", "--dirty"], capture_output=True, text=True)
        version = f"{tool} at {g.stdout.strip() or 'unknown commit'}"
    results = {}
    for name, f, _, _ in probes():
        with tempfile.TemporaryDirectory() as tmp:
            try:
                res = subprocess.run(compile_cmd(str(f)), capture_output=True, text=True, cwd=tmp, timeout=300)
            except subprocess.TimeoutExpired:
                results[name] = {"accepts": None, "message": "timed out"}
                continue
        ok = accepted(tool, res)
        rec = {"accepts": ok}
        if not ok:
            rec["message"] = first_error(res.stdout + res.stderr)
        results[name] = rec
        print(f"{tool} {name}: {'accepts' if ok else 'rejects'}")
    out_dir.mkdir(parents=True, exist_ok=True)
    table = {"tool": tool, "version": version, "measured": datetime.date.today().isoformat(),
             "_doc": "Measured by tools/quality/vendor_table.py on testdata/vendor; do not edit by hand.",
             "probes": results}
    (out_dir / f"{tool}.json").write_text(json.dumps(table, indent=1) + "\n")
    return 0


def table(dirs):
    tabs = [json.loads(p.read_text()) for p in sorted(dirs.glob("*.json"))]
    head = "| Probe | slang option | " + " | ".join(t["tool"] for t in tabs) + " |"
    print(head)
    print("|" + "---|" * (2 + len(tabs)))
    mark = {True: "yes", False: "no", None: "?"}
    for name, _, flag, _ in probes():
        cells = [mark[t["probes"].get(name, {}).get("accepts")] for t in tabs]
        print(f"| `{name}` | `{flag}` | " + " | ".join(cells) + " |")
    print()
    for t in tabs:
        print(f"- {t['tool']}: {t['version']} (measured {t['measured']})")
    return 0


def slang_flags(path):
    t = json.loads(Path(path).read_text())
    flags = [flag for name, _, flag, _ in probes()
             if flag != "none" and t["probes"].get(name, {}).get("accepts")]
    print(" ".join(flags))
    return 0


def main():
    a = sys.argv[1:]
    if a[:1] == ["measure"] and len(a) >= 2 and a[1] in TOOLS:
        out = Path(a[a.index("--out") + 1]) if "--out" in a else OUT
        return measure(a[1], out)
    if a[:1] == ["table"]:
        return table(Path(a[1]) if len(a) > 1 else OUT)
    if a[:1] == ["slang-flags"] and len(a) == 2:
        return slang_flags(a[1])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
