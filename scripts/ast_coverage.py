#!/usr/bin/env python3
"""
How much of slang's AST the rules inspect (docs/conformance.md).

For each kind of symbol, statement, expression, timing control, assertion
expression and constraint that slang defines, the kind counts as inspected when
rule or engine code names it: a handler or visitor taking it (handle(const
XSymbol&)), a cast to it (as<XSymbol>), or a kind check (SymbolKind::X). It is a
source-level count, not a run-time one, so it is cheap and exact about what the
code mentions, and blind to kinds reached only through generic walks.

Plain type symbols (IntegralType, StringType, ...) carry no testbench behavior of
their own and are listed separately.

  scripts/ast_coverage.py [--slang-include DIR] [--json OUT] [--floor FILE]

--floor fails when a category inspects fewer kinds than the floor records, so
coverage only goes up.
"""

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CODE_DIRS = ["cpp/src", "cpp/include", "python"]
CATEGORIES = [
    # (macro in slang headers, kind enum, class-name suffix)
    ("SYMBOLKIND", "SymbolKind", "Symbol"),
    ("STATEMENT", "StatementKind", "Statement"),
    ("EXPRESSION", "ExpressionKind", "Expression"),
    ("CONTROL", "TimingControlKind", "Control"),
    ("EXPR", "AssertionExprKind", "AssertionExpr"),
    ("CONSTRAINT", "ConstraintKind", "Constraint"),
]


def find_slang_include():
    for cand in [REPO / "build" / "_deps" / "slang-src" / "include", REPO / "build-dev" / "_deps" / "slang-src" / "include"]:
        if (cand / "slang" / "ast" / "Symbol.h").is_file():
            return cand
    return None


def enum_members(text, macro):
    m = re.search(r"#define " + macro + r"\(x\)((?:.*\\\n)*.*)", text)
    return re.findall(r"x\((\w+)\)", m.group(1)) if m else []


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--slang-include", help="slang's include directory (default: the build's _deps copy)")
    ap.add_argument("--json", help="write the result as JSON to this file")
    ap.add_argument("--floor", help="fail when any category is below the counts in this JSON file")
    args = ap.parse_args()

    inc = Path(args.slang_include) if args.slang_include else find_slang_include()
    if not inc:
        print("slang headers not found; build first or pass --slang-include")
        return 2
    headers = "".join(p.read_text(errors="replace") for p in (inc / "slang" / "ast").rglob("*.h"))
    code = "".join(p.read_text(errors="replace")
                   for d in CODE_DIRS for p in (REPO / d).rglob("*") if p.suffix in (".cpp", ".h", ".py"))

    named = set(re.findall(r"\b(?:Symbol|Statement|Expression|TimingControl|AssertionExpr|Constraint)Kind::(\w+)", code))
    typed = set(re.findall(r"(?:handle|visit|on\w*)\(\s*const\s+(?:slang::)?(?:ast::)?(\w+)\s*&", code))
    typed |= set(re.findall(r"\bas<\s*(?:const\s+)?(?:slang::)?(?:ast::)?(\w+)\s*>", code))
    typed |= set(re.findall(r"\bas_if<\s*(?:const\s+)?(?:slang::)?(?:ast::)?(\w+)\s*>", code))

    result = {}
    print(f"{'category':18} {'inspected':>9} {'of':>4}  not inspected")
    for macro, enum, suffix in CATEGORIES:
        kinds = [k for k in enum_members(headers, macro) if k not in ("Unknown", "Invalid")]
        if not kinds:
            continue
        def hit(k):
            return k in named or k + suffix in typed or k in typed or (k + "Type") in typed
        types = [k for k in kinds if enum == "SymbolKind" and (k.endswith("Type") or k in ("ScalarType",))]
        meaningful = [k for k in kinds if k not in types]
        inspected = [k for k in meaningful if hit(k)]
        missing = [k for k in meaningful if not hit(k)]
        result[enum] = {"inspected": len(inspected), "total": len(meaningful), "missing": missing}
        print(f"{enum:18} {len(inspected):>9} {len(meaningful):>4}  {' '.join(missing)}")

    if args.json:
        Path(args.json).write_text(json.dumps(result, indent=2) + "\n")
    if args.floor:
        floor = json.loads(Path(args.floor).read_text())
        low = [f"{k}: {result.get(k, {}).get('inspected', 0)} < floor {v}" for k, v in floor.items()
               if not k.startswith("_") and result.get(k, {}).get("inspected", 0) < v]
        if low:
            print("FAILED: AST coverage went down:\n  " + "\n  ".join(low))
            return 1
        print("PASSED: no category is below its floor")
    return 0


if __name__ == "__main__":
    sys.exit(main())
