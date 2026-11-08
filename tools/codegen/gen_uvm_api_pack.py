#!/usr/bin/env python3
"""
Generates a UVM API pack (data/uvm-api/<version>.json) from a uvm-core checkout.

uvm-core marks its API in comments: `@uvm-ieee 1800.2-2020 auto 5.3.4.1` on
members the IEEE 1800.2 standard defines, `@uvm-accellera` on details of the
Accellera implementation that are not part of the standard, and `@uvm-contrib`
on items contributed for a future revision. The tag sits on the comment line
just before the declaration. This script pairs each tag with the declaration
that follows, records the class it belongs to, and lists the library's macros,
marking those under src/deprecated (compiled only with UVM_ENABLE_DEPRECATED_API).

  tools/codegen/gen_uvm_api_pack.py <uvm-core checkout> [--out data/uvm-api]

UVM 1.1d and 1.2 predate the tags: their packs list the library's macros and
the methods it deprecates. Deprecated API there is what the library drops
when UVM_NO_DEPRECATED is defined (`ifndef UVM_NO_DEPRECATED regions, and
src/deprecated); in UVM 2020 it is what needs UVM_ENABLE_DEPRECATED_API.
A pack's "deprecatedGuard" names that define.

A pack is plain data: rules read it through uvm::ApiPack (C++) and
dv-lint's uvm_api (Rust), so supporting a new UVM release is running this
script on it, with no code change.
"""

import argparse
import json
import re
import sys
from pathlib import Path

TAG = re.compile(r"//\s*@uvm-(ieee|accellera|contrib)\b\s*([0-9.\-]*)\s*(?:auto|manual)?\s*([0-9A-Z.]*)")
CLASS = re.compile(r"^\s*(?:virtual\s+|interface\s+)?class\s+(?::\w+\s+)?(\w+)")
ENDCLASS = re.compile(r"^\s*endclass\b")
MEMBER = re.compile(r"^\s*(?:extern\s+|pure\s+|virtual\s+|static\s+|protected\s+|local\s+|const\s+)*"
                    r"(function|task)\b(?:\s+automatic)?(?:\s+static)?\s+(?:[\w:#\[\]$ ]+?\s+)?(\w+)\s*[(;]")
OUT_OF_BLOCK = re.compile(r"^\s*(?:function|task)\b.*?\b(\w+)\s*(?:#\(.*?\))?::(\w+)\s*[(;]")
TYPEDEF = re.compile(r"^\s*typedef\s+.*?(\w+)\s*;")
VERSION = re.compile(r'UVM_VERSION_STRING\s*=\s*"([^"]+)"')
MACRO = re.compile(r"^\s*`define\s+(uvm_\w+|UVM_\w+)")
COND = re.compile(r"^\s*`(ifdef|ifndef|elsif|else|endif)\b\s*(\w*)")
# The define that deprecated API hangs on, and whether it is tested with
# `ifndef (1.x: defining it removes the API) or `ifdef (2020: defining it adds it).
GUARDS = {"UVM_NO_DEPRECATED": "ifndef", "UVM_ENABLE_DEPRECATED_API": "ifdef"}
REV = re.compile(r"^\s*`define\s+UVM_(MAJOR|MINOR|FIX)_REV\s+(\w+)", re.M)


def scan(src: Path):
    classes, macros = {}, {}
    for f in sorted(src.rglob("*.sv*")):
        rel = f.relative_to(src).as_posix()
        deprecated = rel.startswith("deprecated/")
        lines = f.read_text(errors="replace").splitlines()
        current = None
        pending = None
        # One entry per open `ifdef: is its live branch deprecated API?
        conds = []
        for line in lines:
            k = COND.match(line)
            if k:
                d, name = k.group(1), k.group(2)
                if d in ("ifdef", "ifndef"):
                    conds.append(GUARDS.get(name) == d)
                elif d in ("else", "elsif") and conds:
                    conds[-1] = False
                elif d == "endif" and conds:
                    conds.pop()
                continue
            in_dep = deprecated or any(conds)
            m = MACRO.match(line)
            if m:
                macros.setdefault(m.group(1), {"file": rel, "deprecated": in_dep})
            t = TAG.search(line)
            if t:
                pending = {"tag": t.group(1), "standard": t.group(2) or None, "clause": t.group(3) or None}
                continue
            stripped = line.strip()
            if ENDCLASS.match(line):
                current = None
                pending = None if pending and not stripped.startswith("//") else pending
                continue
            c = CLASS.match(line)
            if c and not stripped.startswith("//"):
                current = c.group(1)
                entry = classes.setdefault(current, {"members": {}})
                entry.setdefault("file", rel)
                if in_dep:
                    entry["deprecated"] = True
                if pending:
                    entry.update({k: v for k, v in pending.items() if v})
                    pending = None
                continue
            if not stripped or stripped.startswith("//") or stripped.startswith("`"):
                continue
            if pending or (in_dep and current):
                o = OUT_OF_BLOCK.match(line)
                m = MEMBER.match(line)
                cls, name = (o.group(1), o.group(2)) if o else (current, m.group(2) if m else None)
                if not name:
                    td = TYPEDEF.match(line)
                    name = td.group(1) if td and current else None
                if cls and name:
                    members = classes.setdefault(cls, {"members": {}})["members"]
                    rec = {k: v for k, v in (pending or {}).items() if v}
                    if in_dep:
                        rec["deprecated"] = True
                    # Keep the first tag: an out-of-block body repeats the prototype's.
                    members.setdefault(name, rec)
                pending = None
    return classes, macros


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("uvm_core", help="uvm-core checkout (or its src/)")
    ap.add_argument("--out", default=str(Path(__file__).resolve().parents[2] / "data" / "uvm-api"))
    args = ap.parse_args()
    root = Path(args.uvm_core)
    src = root / "src" if (root / "src" / "uvm_pkg.sv").is_file() else root
    if not (src / "uvm_pkg.sv").is_file():
        print(f"{root} is not a uvm-core checkout (no uvm_pkg.sv)")
        return 2
    vtext = (src / "base" / "uvm_version.svh").read_text(errors="replace") if (src / "base" / "uvm_version.svh").is_file() else ""
    vm = VERSION.search(vtext)
    version_string = vm.group(1) if vm else "unknown"
    parts = version_string.split(":")
    version = parts[-1]
    # "Accellera:1800.2-2020:UVM:2.0" names the 2020 line's release 2.0 as just "2.0".
    if not version.startswith(("20", "1.")) and len(parts) > 1 and "-" in parts[1]:
        version = parts[1].split("-")[-1] + "-" + version
    if version_string == "unknown":
        # UVM 1.x builds its string from macros: "UVM-1.1d".
        dtext = (src / "macros" / "uvm_version_defines.svh").read_text(errors="replace") \
            if (src / "macros" / "uvm_version_defines.svh").is_file() else ""
        rev = dict(REV.findall(dtext))
        if "MAJOR" in rev and "MINOR" in rev:
            version = f"{rev['MAJOR']}.{rev['MINOR']}{rev.get('FIX', '')}"
            version_string = "UVM-" + version
    classes, macros = scan(src)
    counts = {"ieee": 0, "accellera": 0, "contrib": 0}
    for c in classes.values():
        if c.get("tag"):
            counts[c["tag"]] += 1
        for mem in c["members"].values():
            if mem.get("tag"):
                counts[mem["tag"]] += 1
    pack = {
        "_doc": "Generated by tools/codegen/gen_uvm_api_pack.py from uvm-core; do not edit by hand.",
        "version": version,
        "versionString": version_string,
        "deprecatedGuard": "UVM_NO_DEPRECATED" if version.startswith("1.") else "UVM_ENABLE_DEPRECATED_API",
        "counts": counts,
        "classes": {k: classes[k] for k in sorted(classes)},
        "macros": {k: macros[k] for k in sorted(macros)},
    }
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{version}.json"
    path.write_text(json.dumps(pack, indent=1, sort_keys=False) + "\n")
    print(f"{path}: {len(classes)} classes, {counts}, {len(macros)} macros "
          f"({sum(1 for m in macros.values() if m['deprecated'])} deprecated), "
          f"{sum(1 for c in classes.values() for m in c['members'].values() if m.get('deprecated'))} deprecated methods")
    return 0


if __name__ == "__main__":
    sys.exit(main())
