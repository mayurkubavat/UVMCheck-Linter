#!/usr/bin/env python3
"""
Invariants of the --fix pipeline, checked on every rule test file and on mutated copies.

For each testdata/rules/<category>/<RULE>/{bad,good}.sv that has a fix to apply, a copy (with its
own copy of testdata/common, since --fix also rewrites included files) is
fixed with `--fix --unsafe-fixes --preset all --experimental` and must:
  1. finish (no crash, no hang) within the timeout;
  2. not add compile errors: the fixed file has no more slang errors than the original;
  3. reach a fixed point in one run: a second --fix changes nothing (passes repeat inside
     one invocation until the text stops changing);
  4. keep every macro call (`name) the original wrote.

Mutation mode (--mutations N, default 3 per file in a sample) also feeds broken copies
(dropped, duplicated or truncated lines, chosen with a fixed seed) through --fix and
checks only that the linter finishes without crashing or hanging. Mutated text may not
compile, so the compile-error invariant does not apply to it.
"""

import argparse
import json
import random
import re
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

TESTS_DIR = Path(__file__).resolve().parent
DATA_DIR = Path(__file__).resolve().parent.parent / "testdata"
RULES_DIR = DATA_DIR / "rules"
COMMON_DIR = DATA_DIR / "common"
MACRO_CALL = re.compile(r"`[A-Za-z_]\w*")
FIX_ARGS = ["--fix", "--unsafe-fixes", "--preset", "all", "--experimental"]


def lint(binary, work, name, extra, timeout):
    """Runs the linter on work/name; returns (returncode, report dict or None, stdout)."""
    report = work / "report.json"
    report.unlink(missing_ok=True)
    # A private copy of testdata/common: --fix also rewrites included files.
    common = work / "common"
    if not common.is_dir():
        shutil.copytree(COMMON_DIR, common)
    cmd = [binary, name, "-I", str(common), "--no-config", "--no-cache",
           "--json", str(report), *extra]
    res = subprocess.run(cmd, cwd=work, capture_output=True, text=True, timeout=timeout)
    data = None
    if report.is_file():
        try:
            data = json.loads(report.read_text())
        except json.JSONDecodeError:
            data = None
    return res.returncode, data, res.stdout + res.stderr


def compile_errors(report):
    n = (report or {}).get("compileErrors") or 0
    return n if isinstance(n, int) else len(n)


def fixable(report):
    for v in (report or {}).get("violations", []):
        for f in v.get("fixes", []):
            if f.get("safety") != "display-only":
                return True
    return False


def crashed(rc):
    # Negative: killed by a signal. Above 4: not one of the linter's exit codes.
    return rc < 0 or rc > 4


def check_file(binary, src, timeout):
    """Returns a list of failure messages for one test file."""
    failures = []
    label = f"{src.parent.parent.name}/{src.parent.name}/{src.name}"
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for helper in src.parent.iterdir():
            if helper.is_file():
                shutil.copy(helper, work / helper.name)
        original = (work / src.name).read_text()

        try:
            rc, before, _ = lint(binary, work, src.name, ["--preset", "all", "--experimental"], timeout)
        except subprocess.TimeoutExpired:
            return [f"{label}: lint timed out"]
        if crashed(rc):
            return [f"{label}: lint crashed (exit {rc})"]
        if not fixable(before):
            return []

        try:
            rc, after, out = lint(binary, work, src.name, FIX_ARGS, timeout)
        except subprocess.TimeoutExpired:
            return [f"{label}: --fix timed out"]
        if crashed(rc):
            return [f"{label}: --fix crashed (exit {rc})"]
        if "undo each other" in out or "still changing" in out:
            failures.append(f"{label}: --fix did not settle: {out.strip().splitlines()[-1]}")

        fixed = (work / src.name).read_text()
        if compile_errors(after) > compile_errors(before):
            failures.append(f"{label}: --fix added compile errors "
                            f"({compile_errors(before)} -> {compile_errors(after)})")
        lost = set(MACRO_CALL.findall(original)) - set(MACRO_CALL.findall(fixed))
        if lost:
            failures.append(f"{label}: --fix removed macro call(s) {sorted(lost)}")

        try:
            lint(binary, work, src.name, FIX_ARGS, timeout)
        except subprocess.TimeoutExpired:
            return failures + [f"{label}: second --fix timed out"]
        if (work / src.name).read_text() != fixed:
            failures.append(f"{label}: a second --fix still changed the file")
    return failures


def mutate(text, rng):
    lines = text.splitlines(keepends=True)
    if len(lines) < 3:
        return text
    i = rng.randrange(len(lines))
    kind = rng.choice(["drop", "dup", "truncate"])
    if kind == "drop":
        del lines[i]
    elif kind == "dup":
        lines.insert(i, lines[i])
    else:
        line = lines[i].rstrip("\n")
        lines[i] = line[: rng.randrange(len(line) + 1)] + "\n"
    return "".join(lines)


def check_mutations(binary, src, count, seed, timeout):
    failures = []
    rng = random.Random(f"{seed}:{src}")
    label = f"{src.parent.parent.name}/{src.parent.name}/{src.name}"
    for n in range(count):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            for helper in src.parent.iterdir():
                if helper.is_file():
                    shutil.copy(helper, work / helper.name)
            (work / src.name).write_text(mutate(src.read_text(), rng))
            try:
                rc, _, _ = lint(binary, work, src.name, FIX_ARGS, timeout)
            except subprocess.TimeoutExpired:
                failures.append(f"{label} mutation {n}: --fix timed out")
                continue
            if crashed(rc):
                failures.append(f"{label} mutation {n}: --fix crashed (exit {rc})")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--mutations", type=int, default=3, help="mutated copies per sampled file")
    parser.add_argument("--sample", type=int, default=40, help="files to mutate (0 = all)")
    parser.add_argument("--seed", default="uvmcheck")
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("-j", "--jobs", type=int, default=8)
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())

    files = sorted(p for p in RULES_DIR.glob("*/*/*.sv") if p.name in ("bad.sv", "good.sv"))
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        failures = [f for r in pool.map(lambda p: check_file(binary, p, args.timeout), files) for f in r]
        sample = files
        if args.sample:
            sample = random.Random(args.seed).sample(files, min(args.sample, len(files)))
        if args.mutations:
            failures += [f for r in pool.map(
                lambda p: check_mutations(binary, p, args.mutations, args.seed, args.timeout), sample) for f in r]

    for f in failures:
        print(f"  [FAIL] {f}")
    print(f"Fix invariants: {len(files)} files, {len(sample) * args.mutations if args.mutations else 0} mutations, "
          f"{len(failures)} failure(s).")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
