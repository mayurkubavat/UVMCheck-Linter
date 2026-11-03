#!/usr/bin/env python3
"""Lint the same sources under several preprocessor configurations.

slang, like every compiler, sees one configuration per run: code inside an
`ifdef whose macro is not defined is never parsed, so no rule checks it. This
runs uvmcheck once with the arguments as given, then once per configuration,
and merges the findings. A finding that appears only under some configurations
says which ones.

Configurations come from --set (repeatable, comma-separated defines, e.g.
--set "FAST_SIM,WIDTH=64") and, with --auto, one per macro the sources test
with `ifdef / `ifndef / `elsif and never `define themselves.

  scripts/config_sweep.py --binary build/uvmcheck --auto -- tb.sv -I inc
  scripts/config_sweep.py --binary build/uvmcheck --set A --set "B,C=2" --json out.json -- -f files.f

Exit status: 1 when any configuration reports an error-severity finding (or
does not run), 0 otherwise.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

COND = re.compile(r"`(?:ifdef|ifndef|elsif)\s+(\w+)")
DEFINE = re.compile(r"`define\s+(\w+)")
SOURCE_EXT = {".sv", ".svh", ".v", ".vh"}


def source_files(args):
    """The source files named in uvmcheck's arguments, with -f/-F file lists expanded one level."""
    files, it = [], iter(args)
    for a in it:
        if a in ("-f", "-F"):
            lst = Path(next(it, ""))
            if lst.is_file():
                for line in lst.read_text(errors="replace").splitlines():
                    line = line.split("//")[0].strip()
                    if line and not line.startswith(("-", "+")):
                        p = Path(line) if Path(line).is_absolute() else lst.parent / line
                        if p.suffix in SOURCE_EXT:
                            files.append(p)
        elif not a.startswith(("-", "+")) and Path(a).suffix in SOURCE_EXT:
            files.append(Path(a))
    return files


def include_dirs(args):
    dirs, it = [], iter(args)
    for a in it:
        if a in ("-I", "--incdir", "-i"):
            dirs.append(Path(next(it, ".")))
        elif a.startswith("+incdir+"):
            dirs += [Path(d) for d in a[len("+incdir+"):].split("+") if d]
    return dirs


def auto_macros(args):
    """Macros the user's sources test but never define, in first-seen order."""
    files = source_files(args)
    for d in include_dirs(args):
        if d.is_dir():
            files += sorted(p for p in d.iterdir() if p.suffix in SOURCE_EXT)
    tested, defined = [], set()
    for f in files:
        # The UVM library's own switches are not the user's configurations.
        if f.name.startswith("uvm_") or not f.is_file():
            continue
        text = f.read_text(errors="replace")
        defined.update(DEFINE.findall(text))
        for m in COND.findall(text):
            if m not in tested:
                tested.append(m)
    return [m for m in tested if m not in defined and not m.startswith("UVM_")]


def run(binary, args, defines):
    cmd = [binary, "--json", "-", *args]
    for d in defines:
        cmd += ["-D", d]
    res = subprocess.run(cmd, capture_output=True, text=True)
    try:
        return json.loads(res.stdout), res.returncode, res.stderr
    except json.JSONDecodeError:
        return None, res.returncode, res.stderr + res.stdout


def key(v):
    return (v["ruleId"], v["file"], v["line"], v["column"], v["message"])


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--binary", default="build/uvmcheck")
    ap.add_argument("--set", action="append", default=[], help="a configuration: comma-separated defines")
    ap.add_argument("--auto", action="store_true", help="add one configuration per `ifdef macro the sources test")
    ap.add_argument("--max", type=int, default=32, help="most configurations to run (default 32)")
    ap.add_argument("--json", help="write the merged report here")
    ap.add_argument("args", nargs=argparse.REMAINDER, help="-- then uvmcheck's arguments")
    opts = ap.parse_args()
    args = opts.args[1:] if opts.args[:1] == ["--"] else opts.args

    configs = [("default", [])]
    for s in opts.set:
        configs.append((s, [d.strip() for d in s.split(",") if d.strip()]))
    if opts.auto:
        for m in auto_macros(args):
            if not any(c[1] == [m] for c in configs):
                configs.append((m, [m]))
    configs = configs[: opts.max + 1]

    merged, seen_in, failed = {}, {}, []
    for name, defines in configs:
        rep, rc, err = run(opts.binary, args, defines)
        if rep is None:
            failed.append(name)
            print(f"[config-sweep] {name}: uvmcheck did not produce a report (rc={rc})\n{err.strip()}", file=sys.stderr)
            continue
        for v in rep.get("violations", []):
            if v["file"].startswith("<"):  # about the -D itself, not the sources
                continue
            k = key(v)
            merged.setdefault(k, v)
            seen_in.setdefault(k, []).append(name)

    names = [c[0] for c in configs]
    violations = []
    for k, v in merged.items():
        where = seen_in[k]
        v = dict(v)
        if len(where) < len(names) - len(failed):
            v["configurations"] = where
        violations.append(v)
    violations.sort(key=lambda v: (v["file"], v["line"], v["column"], v["ruleId"]))

    only = [v for v in violations if "configurations" in v and "default" not in v["configurations"]]
    print(f"[config-sweep] {len(names)} configuration(s): {', '.join(names)}")
    print(f"[config-sweep] {len(violations)} finding(s), {len(only)} only outside the default configuration")
    for v in only:
        print(f"  {v['file']}:{v['line']}:{v['column']}: {v['ruleId']} [{', '.join(v['configurations'])}] "
              f"{v['message'][:140]}")
    if opts.json:
        out = {"configurations": [{"name": n, "defines": d} for n, d in configs], "failed": failed,
               "violations": violations}
        Path(opts.json).write_text(json.dumps(out, indent=2) + "\n")
    errors = any(v.get("severity", "").lower() == "error" for v in violations)
    return 1 if failed or errors else 0


if __name__ == "__main__":
    sys.exit(main())
