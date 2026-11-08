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

  tools/quality/config_sweep.py --binary build/uvmcheck --auto -- tb.sv -I inc
  tools/quality/config_sweep.py --binary build/uvmcheck --set A --set "B,C=2" --json out.json -- -f files.f

--specializations adds the specialization sweep. A compiler checks a
parameterized class once per specialization the design names, so code that
depends on a type parameter is checked only for the types actually used (and
the default). The sweep adds one run per stand-in type (int, string,
logic [7:0], real, and a class handle when UVM is compiled): a generated
module declares a handle of every generic class the user's sources define,
with each type parameter set to that type and value parameters left at their
defaults. Errors that appear only then are code that breaks for some type,
e.g. `v + 1` on a `type T` that could be a string.

  tools/quality/config_sweep.py --binary build/uvmcheck --specializations -- -f files.f

Exit status: 1 when any configuration reports an error-severity finding (or
does not run), 0 otherwise.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

COND = re.compile(r"`(?:ifdef|ifndef|elsif)\s+(\w+)")
DEFINE = re.compile(r"`define\s+(\w+)")
SOURCE_EXT = {".sv", ".svh", ".v", ".vh"}


def list_args(args, depth=0):
    """The arguments with -f/-F file lists read in (nested lists too) and
    $VAR / ${VAR} expanded, as a compiler reads them."""
    out, it = [], iter(args)
    for a in it:
        if a in ("-f", "-F") and depth < 8:
            lst = Path(os.path.expandvars(next(it, "")))
            if lst.is_file():
                words = []
                for line in lst.read_text(errors="replace").splitlines():
                    words += line.split("//")[0].split()
                rel = []
                for w in words:
                    w = os.path.expandvars(w)
                    p = Path(w)
                    if not w.startswith(("-", "+")) and not p.is_absolute():
                        w = str(lst.parent / w)
                    elif w.startswith("+incdir+"):
                        w = "+incdir+" + "+".join(d if Path(d).is_absolute() else str(lst.parent / d)
                                                  for d in w[len("+incdir+"):].split("+") if d)
                    rel.append(w)
                out += list_args(rel, depth + 1)
        else:
            out.append(os.path.expandvars(a))
    return out


def source_files(args):
    """The source files named in uvmcheck's arguments, file lists read in."""
    return [Path(a) for a in list_args(args) if not a.startswith(("-", "+")) and Path(a).suffix in SOURCE_EXT]


def include_dirs(args):
    dirs, it = [], iter(list_args(args))
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


CLASS_HEAD = re.compile(r"\b(?:virtual\s+)?class\s+(?::\w+\s+)?(\w+)\s*#\s*\(", re.S)
PACKAGE = re.compile(r"\bpackage\s+(\w+)\s*;|\bendpackage\b")
UNIT_END = re.compile(r"\b(?:module|interface|program|class|endclass|endmodule|endinterface|endprogram)\b")
COMMENTS = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)

BUILTIN = re.compile(r"^(?:bit|logic|reg|byte|shortint|int|longint|integer|time|string|real|shortreal|realtime)"
                     r"(?:\s+(?:signed|unsigned))?(?:\s*\[[^\]]*\])*$")

# Stand-in types for type parameters, one sweep run each.
SWEEP_TYPES = [("int", "int"), ("string", "string"), ("logic8", "logic [7:0]"), ("real", "real")]
SWEEP_CLASS = ("object", "uvm_pkg::uvm_object")


def split_top(text):
    """Splits a parameter list at its top-level commas."""
    out, depth, cur = [], 0, ""
    for ch in text:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    return out


def balanced(text, i):
    """The text inside the parentheses that open at text[i]."""
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "(":
            depth += 1
        elif text[j] == ")":
            depth -= 1
            if depth == 0:
                return text[i + 1:j]
    return None


INCLUDE = re.compile(r'`include\s+"([^"]+)"')


def user_texts(args):
    """(file, text, package it is read into) for the user's sources and the
    files they `include (recursively), UVM's own files left out. A file
    included inside `package P; ... endpackage` belongs to P."""
    dirs = include_dirs(args)
    out, seen = [], set()

    def visit(f, pkg):
        try:
            key = f.resolve()
        except OSError:
            return
        if key in seen or not f.is_file() or f.name.startswith("uvm_"):
            return
        seen.add(key)
        text = COMMENTS.sub(" ", f.read_text(errors="replace"))
        out.append((f, text, pkg))
        for m in INCLUDE.finditer(text):
            here = pkg
            for p in PACKAGE.finditer(text[:m.start()]):
                here = p.group(1)
            for d in [f.parent, *dirs]:
                c = d / m.group(1)
                if c.is_file():
                    visit(c, here)
                    break

    for f in source_files(args):
        visit(f, None)
    return out


def generic_classes(args):
    """(scope, class, [(param, is_type, has_default)]) for every parameterized
    class at package or $unit level in the user's sources."""
    found, have = [], set()
    for f, text, outer in user_texts(args):
        for m in CLASS_HEAD.finditer(text):
            before = text[:m.start()]
            pkg = outer
            for p in PACKAGE.finditer(before):
                pkg = p.group(1)
            # Only classes directly in a package or $unit: not nested in a
            # module or another class.
            opened = 0
            for u in UNIT_END.finditer(before):
                w = u.group(0)
                opened += -1 if w.startswith("end") else 1
            if opened != 0 or re.search(r"\binterface\s*$", before[-20:]):
                continue
            inner = balanced(text, m.end() - 1)
            if inner is None:
                continue
            params, kind_type = [], False
            for item in split_top(inner):
                item = item.strip()
                if not item:
                    continue
                words = re.match(r"^(?:parameter\s+|localparam\s+)?(type\s+)?(.*?)(\w+)\s*(?:\[[^\]]*\]\s*)*(=.*)?$",
                                 item, re.S)
                if not words:
                    continue
                if words.group(1):
                    kind_type = True
                elif words.group(2).strip() or item.startswith(("parameter", "localparam")):
                    # A data type or `parameter` starts a value parameter.
                    kind_type = False
                if item.startswith("localparam"):
                    continue
                default = (words.group(4) or "=")[1:].strip()
                # A type parameter defaulting to a class (or any named type)
                # is used as one: it expects that type's members, so a
                # stand-in would only report that they are missing. Such a
                # parameter keeps its default.
                sweep = kind_type and (not default or BUILTIN.match(default) is not None)
                params.append((words.group(3), sweep, bool(words.group(4)), kind_type))
            if any(t for _, t, _, _ in params) and (pkg, m.group(1)) not in have:
                have.add((pkg, m.group(1)))
                found.append((pkg, m.group(1), params))
    return found


def sweep_file(classes, type_text, path):
    lines = ["// Generated by tools/quality/config_sweep.py --specializations.",
             "module __uvmcheck_spec_sweep;"]
    for i, (pkg, cls, params) in enumerate(classes):
        assigns = []
        for name, sweep, has_default, is_type in params:
            if sweep:
                assigns.append(f".{name}({type_text})")
            elif not has_default:
                assigns.append(f".{name}({'int' if is_type else '1'})")
        name = f"{pkg}::{cls}" if pkg else cls
        lines.append(f"  {name} #({', '.join(assigns)}) h{i};")
    lines.append("endmodule")
    path.write_text("\n".join(lines) + "\n")


def run(binary, args, defines, extra=()):
    cmd = [binary, "--json", "-", *args, *extra]
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
    ap.add_argument("--specializations", action="store_true",
                    help="add one run per stand-in type for the type parameters of generic classes")
    ap.add_argument("--max", type=int, default=32, help="most configurations to run (default 32)")
    ap.add_argument("--json", help="write the merged report here")
    ap.add_argument("args", nargs=argparse.REMAINDER, help="-- then uvmcheck's arguments")
    opts = ap.parse_args()
    args = opts.args[1:] if opts.args[:1] == ["--"] else opts.args

    configs = [("default", [], [])]
    for s in opts.set:
        configs.append((s, [d.strip() for d in s.split(",") if d.strip()], []))
    if opts.auto:
        for m in auto_macros(args):
            if not any(c[1] == [m] for c in configs):
                configs.append((m, [m], []))
    tmp = None
    if opts.specializations:
        classes = generic_classes(args)
        print(f"[config-sweep] {len(classes)} generic class(es) to specialize", file=sys.stderr)
        if classes:
            tmp = tempfile.TemporaryDirectory()
            uvm = any("uvm_pkg" in a for a in args) or any("uvm" in a.lower() for a in include_dirs(args) for a in [str(a)])
            for tag, text in SWEEP_TYPES + ([SWEEP_CLASS] if uvm else []):
                f = Path(tmp.name) / f"__uvmcheck_spec_sweep_{tag}.sv"
                sweep_file(classes, text, f)
                configs.append((f"T={text}", [], [str(f)]))
    configs = configs[: opts.max + 1]

    # The sweep reports only in the user's own files: a specialization also
    # reaches library code (UVM), which is not what is being swept.
    user_files = {str(f.resolve()) for f, _, _ in user_texts(args)} if opts.specializations else None
    merged, seen_in, failed = {}, {}, []
    for name, defines, extra in configs:
        rep, rc, err = run(opts.binary, args, defines, extra)
        if rep is None:
            failed.append(name)
            print(f"[config-sweep] {name}: uvmcheck did not produce a report (rc={rc})\n{err.strip()}", file=sys.stderr)
            continue
        for v in rep.get("violations", []):
            if v["file"].startswith("<") or "__uvmcheck_spec_sweep" in v["file"]:
                # About the -D itself or the generated sweep module, not the sources.
                continue
            k = key(v)
            merged.setdefault(k, v)
            where = seen_in.setdefault(k, [])
            if name not in where:
                where.append(name)

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
    if user_files is not None:
        def in_user(v):
            try:
                return str(Path(v["file"]).resolve()) in user_files or not v["configurations"][0].startswith("T=")
            except OSError:
                return True
        violations = [v for v in violations if "configurations" not in v or in_user(v)]
        only = [v for v in only if in_user(v)]
    print(f"[config-sweep] {len(names)} configuration(s): {', '.join(names)}")
    print(f"[config-sweep] {len(violations)} finding(s), {len(only)} only outside the default configuration")
    for v in only:
        print(f"  {v['file']}:{v['line']}:{v['column']}: {v['ruleId']} [{', '.join(v['configurations'])}] "
              f"{v['message'][:140]}")
    if opts.json:
        out = {"configurations": [{"name": n, "defines": d, "extra": e} for n, d, e in configs], "failed": failed,
               "violations": violations}
        Path(opts.json).write_text(json.dumps(out, indent=2) + "\n")
    errors = any(v.get("severity", "").lower() == "error" for v in violations)
    return 1 if failed or errors else 0


if __name__ == "__main__":
    sys.exit(main())
