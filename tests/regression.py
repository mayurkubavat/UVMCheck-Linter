#!/usr/bin/env python3
"""
UVMCheck Per-Rule Test Runner

Each rule has a folder tests/rules/<category>/<RULE-ID>/ holding:
  bad.sv       must trigger the rule
  good.sv      must not trigger the rule
  expect.json  the exact (rule, line) findings bad.sv must produce

A test may also hold helper files that bad.sv or good.sv `include, for rules about
included files. The rule's own findings in a helper are expected with a "file" key in
expect.json; other rules' findings in helpers are not checked.

Findings are read from the linter's --json report, never scraped from the console.

Modes:
  isolated   (default) run only the rule under test (--rule). bad.sv must produce
             exactly the expected lines; good.sv must produce none.
  all-rules  run every rule, as a user would. bad.sv must still produce its expected
             lines, and every other finding in bad.sv or good.sv must be listed in the
             cross-rule baseline (--baseline). A baseline entry that no longer fires is
             reported as stale so the baseline only ever shrinks.

UVM library:
  By default tests compile against the stub in tests/common/uvm_mock.svh. With
  --uvm-home <uvm-core checkout> they compile against the real Accellera library
  instead: uvm_pkg.sv is added as a source and tests/common/real_uvm/uvm_mock.svh
  replaces the stub with `include "uvm_macros.svh" + import uvm_pkg::*. Both
  include tests/common/test_fixtures.svh for the non-UVM helpers tests share.

Compile errors (from slang's --diag-json) are tracked as pseudo-findings named
COMPILE:<code> in the all-rules baselines, so a test file that newly stops
compiling fails the run. Some bad.sv files are illegal SystemVerilog on purpose
(the rule detects the illegal construct); others are broken because the stub or
the file is incomplete. The baseline records both until the broken ones are fixed.
"""

import sys
import os
import json
import argparse
import subprocess
import tempfile
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

# ANSI color codes
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BLUE = "\033[94m"
BOLD = "\033[1m"
RESET = "\033[0m"

TESTS_DIR = Path(__file__).resolve().parent
EXPECT_FILE = "expect.json"


def find_uvmcheck_binary(explicit_path=None):
    if explicit_path and os.path.isfile(explicit_path):
        return os.path.abspath(explicit_path)

    candidates = [
        "build/uvmcheck",
        "build-dev/uvmcheck",
        "../build/uvmcheck",
        "uvmcheck"
    ]
    for c in candidates:
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    return None


class Linter:
    def __init__(self, binary, common_dir, uvm_home=None):
        self.binary = binary
        self.common_dir = Path(common_dir).resolve()
        self.uvm_home = Path(uvm_home).resolve() if uvm_home else None
        # Run from an empty directory so a stray uvmcheck.json in the tree can't leak in
        self.workdir = tempfile.mkdtemp(prefix="uvmcheck-tests-")

    def run(self, sv_file, rule_id=None):
        sv_file = Path(sv_file).resolve()
        fd, json_path = tempfile.mkstemp(suffix=".json", dir=self.workdir)
        os.close(fd)
        fd, diag_path = tempfile.mkstemp(suffix=".diag.json", dir=self.workdir)
        os.close(fd)

        cmd = [self.binary]
        if self.uvm_home:
            uvm_src = self.uvm_home / "src"
            cmd += [str(uvm_src / "uvm_pkg.sv"), str(sv_file),
                    "-I", str(TESTS_DIR / "common" / "real_uvm"), "-I", str(uvm_src),
                    "-I", str(self.common_dir)]
        else:
            cmd += [str(sv_file), "-I", str(self.common_dir)]
        if rule_id:
            cmd += ["--rule", rule_id]
        else:
            cmd += ["--preset", "all"]  # all-rules mode includes the opt-in Advisory rules
        cmd += ["--json", json_path, "--diag-json", diag_path]

        start_time = time.time()
        res = subprocess.run(cmd, capture_output=True, text=True, cwd=self.workdir)
        elapsed = time.time() - start_time

        report = None
        diags = []
        try:
            with open(json_path) as f:
                report = json.load(f)
            with open(diag_path) as f:
                diags = json.load(f)
        except (OSError, json.JSONDecodeError):
            pass
        finally:
            os.unlink(json_path)
            os.unlink(diag_path)

        if report is None:
            return {"error": f"no JSON report (exit code {res.returncode})",
                    "stdout": res.stdout, "stderr": res.stderr, "elapsed": elapsed}

        # Keep findings in the test file only; the real UVM library has thousands of its own.
        # Reported paths are relative to the linter's working directory.
        findings = [(v["ruleId"], v["line"]) for v in report["violations"]
                    if v["file"] and (Path(self.workdir) / v["file"]).resolve() == sv_file]
        # The rule's findings in helper files next to the test, as (rule, "file:line").
        helper_findings = []
        for v in report["violations"]:
            if not v["file"]:
                continue
            path = (Path(self.workdir) / v["file"]).resolve()
            if path.parent == sv_file.parent and path.name not in ("bad.sv", "good.sv"):
                helper_findings.append((v["ruleId"], f"{path.name}:{v['line']}"))

        compile_errors = []
        for d in diags:
            if d.get("severity") not in ("error", "fatal"):
                continue
            path, line = "", 0
            if d.get("location"):
                path, line = d["location"].rsplit(":", 2)[:2]
            name = f"COMPILE:{d.get('code', 'Unknown')}"
            if not path or (Path(self.workdir) / path).resolve() != sv_file:
                name += f":{Path(path).name or '?'}"
            compile_errors.append((name, int(line)))

        return {
            "error": None,
            "compile_errors": compile_errors,
            "findings": findings,
            "helper_findings": helper_findings,
            "stdout": res.stdout,
            "stderr": res.stderr,
            "elapsed": elapsed,
        }


def fmt(findings):
    return ", ".join(f"{r}@{l}" for r, l in sorted(findings, key=lambda f: (f[0], str(f[1])))) or "none"


def load_expect(rule_path):
    p = rule_path / EXPECT_FILE
    if not p.is_file():
        return None
    with open(p) as f:
        data = json.load(f)
    return [(e["rule"], f"{e['file']}:{e['line']}" if "file" in e else e["line"])
            for e in data.get("bad.sv", [])]


def write_expect(rule_path, findings):
    entries = []
    for r, l in sorted(findings, key=lambda f: (isinstance(f[1], str), f[0], str(f[1]))):
        if isinstance(l, str):
            file, line = l.rsplit(":", 1)
            entries.append({"rule": r, "file": file, "line": int(line)})
        else:
            entries.append({"rule": r, "line": l})
    data = {"bad.sv": entries}
    with open(rule_path / EXPECT_FILE, "w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


def check_file(linter, rule_path, file_name, mode, expected, baseline):
    """Returns (failures, observed_cross_findings) for one test file."""
    rule_id = rule_path.name
    key = f"{rule_path.parent.name}/{rule_id}/{file_name}"
    result = linter.run(rule_path / file_name, rule_id=rule_id if mode == "isolated" else None)

    if result["error"]:
        return [f"{key}: {result['error']}\n{result['stderr'].strip()}"], [], result

    failures = []
    findings = result["findings"]
    own = [f for f in findings + result["helper_findings"] if f[0] == rule_id]
    cross = [f for f in findings if f[0] != rule_id] + result["compile_errors"]

    if file_name == "bad.sv":
        if not expected:
            failures.append(f"{key}: {EXPECT_FILE} is missing or lists no findings "
                            f"(bootstrap with --update-expectations)")
        elif Counter(own) != Counter(expected):
            failures.append(f"{key}: expected {fmt(expected)}, got {fmt(own)}")
    elif own:
        failures.append(f"{key}: false positive {fmt(own)}")

    if mode == "all-rules":
        known = Counter(tuple(e.rsplit("@", 1)) for e in baseline.get(key, []))
        known = Counter({(r, int(l)): n for (r, l), n in known.items()})
        unexpected = Counter(cross) - known
        stale = known - Counter(cross)
        if unexpected:
            failures.append(f"{key}: new cross-rule finding(s) {fmt(unexpected.elements())}")
        if stale:
            failures.append(f"{key}: baseline entries no longer fire {fmt(stale.elements())}"
                            f" (shrink it with --update-baseline)")

    return failures, cross, result


def write_step_summary(title, total, failed, failure_list):
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if not path:
        return
    with open(path, "a") as f:
        icon = "✅" if not failed else "❌"
        f.write(f"### {icon} {title}: {total - failed}/{total} passed\n\n")
        if failure_list:
            f.write("<details><summary>Failures</summary>\n\n```\n")
            for msg in failure_list[:300]:
                f.write(msg + "\n")
            if len(failure_list) > 300:
                f.write(f"... and {len(failure_list) - 300} more\n")
            f.write("```\n</details>\n\n")


def main():
    parser = argparse.ArgumentParser(description="UVMCheck Per-Rule Test Runner")
    parser.add_argument("--binary", help="Path to uvmcheck binary")
    parser.add_argument("--rule", help="Filter by rule ID (e.g. UVM-FAC-001)")
    parser.add_argument("--category", help="Filter by category (e.g. uvm, oop, random)")
    parser.add_argument("--mode", choices=["isolated", "all-rules"], default="isolated",
                        help="Run only the rule under test, or every rule")
    parser.add_argument("--uvm-home", help="Compile against this uvm-core checkout instead of the stub")
    parser.add_argument("--baseline", help="Known cross-rule findings for --mode all-rules")
    parser.add_argument("--update-baseline", action="store_true",
                        help="Rewrite --baseline from the current results")
    parser.add_argument("--update-expectations", action="store_true",
                        help="Rewrite each expect.json from the current isolated results")
    parser.add_argument("-j", "--jobs", type=int, default=os.cpu_count() or 1,
                        help="Parallel linter processes")
    parser.add_argument("-v", "--verbose", action="store_true", help="Show passing tests and linter output")
    parser.add_argument("--rules-dir", default=str(TESTS_DIR / "rules"), help="Path to tests/rules directory")
    parser.add_argument("--common-dir", default=str(TESTS_DIR / "common"), help="Path to tests/common include directory")
    args = parser.parse_args()

    binary = find_uvmcheck_binary(args.binary)
    if not binary:
        print(f"{RED}Error: uvmcheck binary not found. Build the project first using 'cmake --build build'{RESET}")
        return 1

    rules_dir = Path(args.rules_dir)
    if not rules_dir.is_dir():
        print(f"{RED}Error: rules directory '{rules_dir}' does not exist.{RESET}")
        return 1
    if args.uvm_home and not (Path(args.uvm_home) / "src" / "uvm_pkg.sv").is_file():
        print(f"{RED}Error: '{args.uvm_home}' is not a uvm-core checkout (no src/uvm_pkg.sv).{RESET}")
        return 1
    if args.update_expectations and args.mode != "isolated":
        print(f"{RED}Error: --update-expectations needs --mode isolated.{RESET}")
        return 1

    baseline = {}
    if args.mode == "all-rules":
        if not args.baseline:
            print(f"{RED}Error: --mode all-rules needs --baseline <file>.{RESET}")
            return 1
        if os.path.isfile(args.baseline):
            with open(args.baseline) as f:
                baseline = json.load(f)

    # Discover test rule folders
    test_dirs = []
    for p in sorted(rules_dir.glob("*/*")):
        if p.is_dir():
            cat = p.parent.name
            rule_id = p.name

            if args.category and cat.lower() != args.category.lower():
                continue
            if args.rule and args.rule.lower() not in rule_id.lower():
                continue
            test_dirs.append(p)

    if not test_dirs:
        print(f"{YELLOW}No matching rule test cases found under {rules_dir}.{RESET}")
        return 0

    library = f"uvm-core ({args.uvm_home})" if args.uvm_home else "UVM stub"
    title = f"Rule tests: {args.mode}, {library}"
    print(f"\n{BOLD}=== {title} ({len(test_dirs)} rules discovered) ==={RESET}\n")

    linter = Linter(binary, args.common_dir, args.uvm_home)
    jobs = []
    for rule_path in test_dirs:
        expected = load_expect(rule_path)
        for name in ("bad.sv", "good.sv"):
            if (rule_path / name).is_file():
                jobs.append((rule_path, name, expected))

    def run_job(job):
        rule_path, name, expected = job
        return check_file(linter, rule_path, name, args.mode, expected, baseline)

    start_time = time.time()
    with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        results = list(pool.map(run_job, jobs))
    elapsed = time.time() - start_time

    all_failures = []
    new_baseline = {}
    failed_tests = 0
    for (rule_path, name, _), (failures, cross, result) in zip(jobs, results):
        key = f"{rule_path.parent.name}/{rule_path.name}/{name}"
        if cross:
            new_baseline[key] = [f"{r}@{l}" for r, l in sorted(cross)]
        if args.update_expectations and name == "bad.sv" and not result.get("error"):
            write_expect(rule_path, [f for f in result["findings"] + result["helper_findings"]
                                      if f[0] == rule_path.name])

        if failures:
            failed_tests += 1
            all_failures.extend(failures)
            print(f"  [{RED}FAIL{RESET}] {key}")
            for msg in failures:
                print(f"         {msg}")
            if args.verbose:
                print(f"{YELLOW}Output:\n{result.get('stdout', '')}{RESET}")
        elif args.verbose:
            print(f"  [{GREEN}PASS{RESET}] {key} ({result['elapsed']*1000:.1f}ms)")

    if args.update_baseline and args.mode == "all-rules":
        with open(args.baseline, "w") as f:
            json.dump(dict(sorted(new_baseline.items())), f, indent=2)
            f.write("\n")
        print(f"{BLUE}Wrote {sum(len(v) for v in new_baseline.values())} known finding(s) "
              f"across {len(new_baseline)} file(s) to {args.baseline}{RESET}")

    total_tests = len(jobs)
    passed_tests = total_tests - failed_tests
    print(f"\n{BOLD}=== Test Results ==={RESET}")
    print(f"Total: {total_tests} | {GREEN}Passed: {passed_tests}{RESET} | "
          f"{RED if failed_tests > 0 else GREEN}Failed: {failed_tests}{RESET} | {elapsed:.1f}s\n")
    write_step_summary(title, total_tests, failed_tests, all_failures)

    if args.update_expectations or (args.update_baseline and args.mode == "all-rules"):
        return 0
    return 0 if failed_tests == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
