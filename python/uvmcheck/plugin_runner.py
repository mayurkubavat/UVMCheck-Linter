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
    module_name = os.path.splitext(os.path.basename(file_path))[0]
    
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
        if issubclass(obj, Rule) and obj is not Rule:
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
    return discovered

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

    for rule_cls in rule_classes:
        try:
            rule_instance = rule_cls()
            rule_instance.analyze(context)
        except Exception as e:
            sys.stderr.write(f"[uvmcheck-python] Exception running rule {rule_cls.__name__}: {e}\n")

    results = [
        {
            "rule_id": v.rule_id,
            "rule_name": v.rule_name,
            "message": v.message,
            "severity": v.severity.value,
            "filename": v.filename,
            "line": v.line,
            "column": v.column
        }
        for v in context.violations
    ]

    print(json.dumps(results, indent=2))

if __name__ == "__main__":
    main()
