#!/usr/bin/env python3
"""
The ERROR budget: how many ERROR findings each rule may report on code that
is known to be clean.

    python3 tools/quality/error_budget.py --binary target/release/uvmcheck [--update [--allow-raise]]

The corpus (tools/quality/precision_corpus.json) is reviewed, simulator-clean
code such as Accellera's UVM library, so an ERROR finding there is almost
always a false positive. tools/quality/error_budget.json records, per corpus
and rule, how many there are today. The gate fails when a rule reports

  * more than its budget (a rule got less precise, or a new rule is wrong
    on real code): the new findings are printed;
  * fewer than its budget: good news that has to be locked in, by running
    with --update.

So the numbers only go down, and the aim is an empty budget: ERROR should
mean a bug a simulator would show. --update rewrites the budget and refuses
to raise a number unless --allow-raise says the new findings were reviewed
and are true positives (say which in the commit).

A corpus whose environment variables are unset is skipped; with none left
the gate fails, so it cannot pass by measuring nothing.
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from precision import expand, lint  # noqa: E402

HERE = Path(__file__).resolve().parent


def errors_by_rule(report):
    by_rule = {}
    for v in report["violations"]:
        if v["severity"] == "ERROR":
            by_rule.setdefault(v["ruleId"], []).append(v)
    return by_rule


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0].strip())
    ap.add_argument("--binary", default="target/release/uvmcheck")
    ap.add_argument("--corpus", default=str(HERE / "precision_corpus.json"))
    ap.add_argument("--budget", default=str(HERE / "error_budget.json"))
    ap.add_argument("--update", action="store_true", help="rewrite the budget from this run")
    ap.add_argument("--allow-raise", action="store_true", help="with --update: accept higher numbers")
    opts = ap.parse_args()

    corpus = json.loads(Path(opts.corpus).read_text())
    budget_path = Path(opts.budget)
    budget = json.loads(budget_path.read_text()) if budget_path.exists() else {}

    failed = False
    measured = 0
    for entry in corpus:
        name = entry["name"]
        args = [expand(a) for a in entry.get("args", [])]
        if None in args:
            print(f"[error-budget] skipping {name}: environment variable not set")
            continue
        measured += 1
        found = errors_by_rule(lint(opts.binary, args))
        allowed = budget.get(name, {})
        total = sum(len(v) for v in found.values())
        print(f"[error-budget] {name}: {total} ERROR findings, budget {sum(allowed.values())}")
        over = {r: v for r, v in found.items() if len(v) > allowed.get(r, 0)}
        under = {r: n for r, n in allowed.items() if len(found.get(r, [])) < n}
        for rule, items in sorted(over.items()):
            print(f"  OVER  {rule}: {len(items)} (budget {allowed.get(rule, 0)})")
            for v in items[:10]:
                print(f"        {v['file']}:{v['line']}: {v['message'][:140]}")
            if len(items) > 10:
                print(f"        ... and {len(items) - 10} more")
        for rule, n in sorted(under.items()):
            print(f"  UNDER {rule}: {len(found.get(rule, []))} (budget {n})")
        if opts.update:
            if over and not opts.allow_raise:
                print("[error-budget] not updated: a number would go up (review the findings; --allow-raise if they are real)")
                failed = True
                continue
            budget[name] = {r: len(v) for r, v in sorted(found.items())}
        elif over:
            failed = True
        elif under:
            print("[error-budget] fewer ERROR findings than the budget: run with --update to lock that in")
            failed = True

    if measured == 0:
        print("[error-budget] no corpus could be measured (set UVM_HOME to a uvm-core checkout)")
        return 1
    if opts.update and not failed:
        budget_path.write_text(json.dumps(budget, indent=2, sort_keys=True) + "\n")
        print(f"[error-budget] wrote {budget_path}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
