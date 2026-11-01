import sys
import os
import json
import argparse
import inspect
import importlib.util
from typing import List, Type
from .rule import Rule, Severity
from .context import Context

def load_rules_from_file(file_path: str) -> List[Type[Rule]]:
    """Dynamically import a Python file and discover all Rule subclasses."""
    rule_classes = []
    # Unique module name, so two rule files with the same base name do not collide.
    module_name = "uvmcheck_rules_" + os.path.splitext(os.path.basename(file_path))[0] + \
        "_" + str(abs(hash(os.path.abspath(file_path))))
    
    spec = importlib.util.spec_from_file_location(module_name, file_path)
    if not spec or not spec.loader:
        return []
    
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
    except Exception as e:
        sys.stderr.write(f"[uvmcheck-python] Error importing {file_path}: {e}\n")
        return []

    for name, obj in inspect.getmembers(module, inspect.isclass):
        # Rules defined in this file (not ones it imports, such as a base rule it extends).
        if issubclass(obj, Rule) and obj is not Rule and obj.__module__ == module_name:
            rule_classes.append(obj)

    return rule_classes

def discover_rules(target_path: str) -> List[Type[Rule]]:
    """Discover rules from a single Python file or an entire directory."""
    discovered = []
    if os.path.isfile(target_path) and target_path.endswith(".py"):
        discovered.extend(load_rules_from_file(target_path))
    elif os.path.isdir(target_path):
        for root, _, files in os.walk(target_path):
            for file in sorted(files):
                if file.endswith(".py") and not file.startswith("__"):
                    discovered.extend(load_rules_from_file(os.path.join(root, file)))
    return leaf_rules(discovered)


def leaf_rules(classes: List[Type[Rule]]) -> List[Type[Rule]]:
    """Drops abstract rules and rules that another discovered rule extends."""
    leaves = []
    for c in classes:
        if c.__dict__.get("abstract", False):
            continue
        if any(o is not c and issubclass(o, c) for o in classes):
            continue
        leaves.append(c)
    return leaves


def main():
    parser = argparse.ArgumentParser(description="UVMCheck Python Rule Plugin Runner")
    parser.add_argument("--rules", required=True, help="Path to Python rule file or directory")
    parser.add_argument("--model", required=True, help="Path to input semantic model JSON file")
    args = parser.parse_args()

    if not os.path.exists(args.model):
        sys.stderr.write(f"[uvmcheck-python] Model file not found: {args.model}\n")
        sys.exit(1)

    with open(args.model, "r", encoding="utf-8") as f:
        try:
            model_data = json.load(f)
        except Exception as e:
            sys.stderr.write(f"[uvmcheck-python] Failed to parse model JSON: {e}\n")
            sys.exit(1)

    rule_classes = discover_rules(args.rules)
    if not rule_classes:
        sys.stderr.write(f"[uvmcheck-python] No Rule subclasses discovered in {args.rules}\n")
        print(json.dumps([]))
        return

    context = Context(model_data)

    # stdout carries the findings JSON; anything a rule prints goes to stderr instead.
    real_stdout = sys.stdout
    sys.stdout = sys.stderr
    failed = False
    try:
        for rule_cls in rule_classes:
            try:
                rule_cls().analyze(context)
            except Exception:
                import traceback
                sys.stderr.write(f"[uvmcheck-python] Exception running rule {rule_cls.__name__}:\n")
                traceback.print_exc()
                failed = True
    finally:
        sys.stdout = real_stdout

    results = [
        {
            "rule_id": v.rule_id,
            "rule_name": v.rule_name,
            "message": v.message,
            "severity": v.severity.value,
            "filename": v.filename,
            "line": v.line,
            "column": v.column,
            "fixes": [
                {
                    "description": f.description,
                    "replacement": f.replacement,
                    "safety": f.safety.value if hasattr(f.safety, "value") else str(f.safety),
                    "line": f.line or v.line,
                    "column": f.column,
                    "endLine": f.end_line,
                    "endColumn": f.end_column,
                }
                for f in v.fixes
            ],
        }
        for v in context.violations
    ]

    print(json.dumps(results, indent=2))
    if failed:
        sys.exit(2)

if __name__ == "__main__":
    main()
