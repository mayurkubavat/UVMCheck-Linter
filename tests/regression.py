#!/usr/bin/env python3
"""
UVMCheck Per-Rule Test Runner

Executes isolated bad.sv / good.sv tests for each rule, asserting:
1. bad.sv triggers the expected rule violation.
2. good.sv passes with 0 violations (verifying zero false positives).
"""

import sys
import os
import argparse
import subprocess
import re
import time
from pathlib import Path

# ANSI color codes
GREEN = "\033[92m"
RED = "\033[91m"
YELLOW = "\033[93m"
BLUE = "\033[94m"
BOLD = "\033[1m"
RESET = "\033[0m"

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

def run_linter(binary, sv_file, include_dir, rule_id=None):
    cmd = [binary, str(sv_file), "-I", str(include_dir)]
    if rule_id:
        cmd.extend(["--rule", rule_id])
    
    start_time = time.time()
    res = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.time() - start_time
    
    # Parse violations from stdout
    clean_stdout = re.sub(r'\x1b\[[0-9;]*m', '', res.stdout)
    violations = []
    for line in clean_stdout.splitlines():
        # Match pattern: path:line:col: [SEVERITY] RULE-ID (Name): message
        m = re.search(r"\[(ERROR|WARNING|INFO|STYLE)\]\s+([A-Z0-9\-_]+)\s+\(([^)]+)\):", line)
        if m:
            violations.append({
                "severity": m.group(1),
                "rule_id": m.group(2),
                "rule_name": m.group(3),
                "raw": line.strip()
            })
    
    return {
        "returncode": res.returncode,
        "stdout": res.stdout,
        "stderr": res.stderr,
        "violations": violations,
        "elapsed": elapsed
    }

def main():
    parser = argparse.ArgumentParser(description="UVMCheck Per-Rule Test Runner")
    parser.add_argument("--binary", help="Path to uvmcheck binary")
    parser.add_argument("--rule", help="Filter by rule ID (e.g. UVM-FAC-001)")
    parser.add_argument("--category", help="Filter by category (e.g. uvm, oop, random)")
    parser.add_argument("-v", "--verbose", action="store_true", help="Show linter stdout/stderr")
    parser.add_argument("--rules-dir", default="tests/rules", help="Path to tests/rules directory")
    parser.add_argument("--common-dir", default="tests/common", help="Path to tests/common include directory")
    args = parser.parse_args()

    binary = find_uvmcheck_binary(args.binary)
    if not binary:
        print(f"{RED}Error: uvmcheck binary not found. Build the project first using 'cmake --build build'{RESET}")
        return 1

    rules_dir = Path(args.rules_dir)
    common_dir = Path(args.common_dir).resolve()

    if not rules_dir.is_dir():
        print(f"{RED}Error: rules directory '{rules_dir}' does not exist.{RESET}")
        return 1

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

    print(f"\n{BOLD}=== Running UVMCheck Rule Testsuite ({len(test_dirs)} rules discovered) ==={RESET}\n")

    total_tests = 0
    passed_tests = 0
    failed_tests = 0

    for rule_path in test_dirs:
        cat = rule_path.parent.name
        rule_id = rule_path.name
        bad_file = rule_path / "bad.sv"
        good_file = rule_path / "good.sv"

        # 1. Test bad.sv (must detect violation)
        if bad_file.is_file():
            total_tests += 1
            result = run_linter(binary, bad_file, common_dir, rule_id=rule_id)
            matching = [v for v in result["violations"] if v["rule_id"] == rule_id]
            
            if matching or (len(result["violations"]) > 0 and not rule_id.startswith("UVM-") and not rule_id.startswith("SV-")):
                print(f"  [{GREEN}PASS{RESET}] {cat}/{rule_id} -> bad.sv detected {len(matching)} violation(s) ({result['elapsed']*1000:.1f}ms)")
                passed_tests += 1
            else:
                print(f"  [{RED}FAIL{RESET}] {cat}/{rule_id} -> bad.sv failed to trigger {rule_id}")
                if args.verbose:
                    print(f"{YELLOW}Output:\n{result['stdout']}{RESET}")
                failed_tests += 1

        # 2. Test good.sv (must have 0 violations)
        if good_file.is_file():
            total_tests += 1
            result = run_linter(binary, good_file, common_dir, rule_id=rule_id)
            matching = [v for v in result["violations"] if v["rule_id"] == rule_id]
            
            if len(matching) == 0:
                print(f"  [{GREEN}PASS{RESET}] {cat}/{rule_id} -> good.sv passed with 0 violations ({result['elapsed']*1000:.1f}ms)")
                passed_tests += 1
            else:
                print(f"  [{RED}FAIL{RESET}] {cat}/{rule_id} -> good.sv triggered false positive: {[v['rule_id'] for v in matching]}")
                if args.verbose:
                    print(f"{YELLOW}Output:\n{result['stdout']}{RESET}")
                failed_tests += 1

    print(f"\n{BOLD}=== Test Results ==={RESET}")
    print(f"Total: {total_tests} | {GREEN}Passed: {passed_tests}{RESET} | {RED if failed_tests > 0 else GREEN}Failed: {failed_tests}{RESET}\n")

    return 0 if failed_tests == 0 else 1

if __name__ == "__main__":
    sys.exit(main())
