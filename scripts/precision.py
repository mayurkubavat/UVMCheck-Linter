#!/usr/bin/env python3
"""
Precision tracking: lint a fixed corpus of real UVM code and compare each
rule's finding count with the previous run.

    python3 scripts/precision.py --binary build/uvmcheck --corpus scripts/precision_corpus.json \\
        --out precision.json [--previous last.json] [--summary summary.md] [--fail-on-jump]

The corpus file lists named code bases:

    [{"name": "uvm-core",
      "args": ["${UVM_HOME}/src/uvm_pkg.sv", "-I", "${UVM_HOME}/src"],
      "count": ["${UVM_HOME}/src"]}]

`args` are passed to uvmcheck (plus --no-config --preset all --experimental
--json); `count` are the folders whose .sv/.svh/.v lines give the kLOC.
${VAR} is expanded from the environment; a corpus whose variables are unset
is skipped.

For every rule the result has its count and findings per kLOC. Against a
previous result a rule is flagged when its count jumps (at least +50% and +5
findings), when it drops to zero from 5 or more (a detection that broke), or,
for a rule new since the previous run, when it reports 2 or more per kLOC.
Experimental rules are listed with their rate so they can be promoted to
stable once they are quiet on real code.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

JUMP_RATIO = 1.5
JUMP_MIN = 5
DROP_MIN = 5
NEW_RULE_RATE = 2.0
SOURCE_SUFFIXES = {".sv", ".svh", ".v", ".vh"}


def expand(value):
    missing = [v for v in re.findall(r"\$\{(\w+)\}", value) if v not in os.environ]
    if missing:
        return None
    return re.sub(r"\$\{(\w+)\}", lambda m: os.environ[m.group(1)], value)


def count_kloc(folders):
    lines = 0
    for folder in folders:
        for path in Path(folder).rglob("*"):
            if path.suffix in SOURCE_SUFFIXES and path.is_file():
                with open(path, errors="replace") as f:
                    lines += sum(1 for line in f if line.strip())
    return lines / 1000.0


def catalog(binary):
    res = subprocess.run([binary, "--list-rules", "--json"], capture_output=True, text=True)
    try:
        return {r["id"]: r for r in json.loads(res.stdout)["rules"]}
    except (json.JSONDecodeError, KeyError):
        return {}


def lint(binary, args):
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "report.json"
        res = subprocess.run([binary, *args, "--no-config", "--preset", "all", "--experimental", "--json", str(out)],
                             capture_output=True, text=True)
        if not out.exists():
            raise RuntimeError(f"uvmcheck failed (exit {res.returncode}):\n{res.stdout[-2000:]}{res.stderr[-2000:]}")
        return json.loads(out.read_text())


def measure(binary, corpus):
    results = {}
    for entry in corpus:
        args = [expand(a) for a in entry.get("args", [])]
        folders = [expand(c) for c in entry.get("count", [])]
        if None in args or None in folders:
            print(f"[precision] skipping {entry['name']}: environment variable not set")
            continue
        report = lint(binary, args)
        kloc = count_kloc(folders) or 1.0
        rules = {}
        for v in report["violations"]:
            rules[v["ruleId"]] = rules.get(v["ruleId"], 0) + 1
        results[entry["name"]] = {
            "kloc": round(kloc, 2),
            "total": len(report["violations"]),
            "compileErrors": report.get("compileErrors", 0),
            "rules": dict(sorted(rules.items())),
        }
        print(f"[precision] {entry['name']}: {len(report['violations'])} findings, {kloc:.1f} kLOC")
    return results


def compare(now, previous):
    """Flags per corpus: (rule, before, after, reason)."""
    flags = {}
    for name, cur in now.items():
        prev = (previous or {}).get(name)
        if not prev:
            continue
        out = []
        for rule in sorted(set(cur["rules"]) | set(prev["rules"])):
            before, after = prev["rules"].get(rule), cur["rules"].get(rule, 0)
            if before is None:
                rate = after / cur["kloc"]
                if rate >= NEW_RULE_RATE:
                    out.append((rule, 0, after, f"new rule, {rate:.1f} per kLOC"))
            elif after >= before * JUMP_RATIO and after - before >= JUMP_MIN:
                out.append((rule, before, after, "jumped"))
            elif before >= DROP_MIN and after == 0:
                out.append((rule, before, after, "dropped to zero"))
        flags[name] = out
    return flags


def summary(now, flags, rules_meta, have_previous):
    lines = ["# Rule precision on real code", ""]
    for name, cur in now.items():
        lines += [f"## {name}", "",
                  f"{cur['total']} findings in {cur['kloc']} kLOC "
                  f"({cur['total'] / cur['kloc']:.1f} per kLOC), {cur['compileErrors']} compile errors.", ""]
        if have_previous:
            f = flags.get(name, [])
            if f:
                lines += ["| Rule | Before | Now | Why |", "| :--- | ---: | ---: | :--- |"]
                lines += [f"| {r} | {b} | {a} | {why} |" for r, b, a, why in f]
            else:
                lines.append("No rule changed noticeably since the previous run.")
            lines.append("")
        top = sorted(cur["rules"].items(), key=lambda kv: -kv[1])[:15]
        lines += ["| Rule | Findings | per kLOC |", "| :--- | ---: | ---: |"]
        lines += [f"| {r} | {n} | {n / cur['kloc']:.2f} |" for r, n in top]
        experimental = [(r, cur["rules"].get(r, 0)) for r, m in rules_meta.items()
                        if m.get("maturity") == "experimental"]
        if experimental:
            lines += ["", "Experimental rules:", "", "| Rule | Findings | per kLOC |", "| :--- | ---: | ---: |"]
            lines += [f"| {r} | {n} | {n / cur['kloc']:.2f} |" for r, n in sorted(experimental)]
        lines.append("")
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0].strip())
    ap.add_argument("--binary", default="build/uvmcheck")
    ap.add_argument("--corpus", default=str(Path(__file__).with_name("precision_corpus.json")))
    ap.add_argument("--out", required=True, help="result JSON to write")
    ap.add_argument("--previous", help="result JSON of the previous run")
    ap.add_argument("--summary", help="Markdown summary to write (also appended to $GITHUB_STEP_SUMMARY)")
    ap.add_argument("--fail-on-jump", action="store_true", help="exit 1 when a rule is flagged")
    args = ap.parse_args()

    binary = str(Path(args.binary).resolve())
    corpus = json.loads(Path(args.corpus).read_text())
    previous = None
    if args.previous and Path(args.previous).is_file():
        previous = json.loads(Path(args.previous).read_text()).get("corpora")

    try:
        now = measure(binary, corpus)
    except RuntimeError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2
    flags = compare(now, previous)
    Path(args.out).write_text(json.dumps({"corpora": now, "flags": {k: [list(f) for f in v] for k, v in flags.items()}},
                                         indent=2) + "\n")

    text = summary(now, flags, catalog(binary), previous is not None)
    if args.summary:
        Path(args.summary).write_text(text)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a") as f:
            f.write(text + "\n")
    flagged = sum(len(v) for v in flags.values())
    print(f"[precision] {flagged} rule(s) flagged" if previous is not None else "[precision] no previous run to compare")
    return 1 if args.fail_on_jump and flagged else 0


if __name__ == "__main__":
    sys.exit(main())
