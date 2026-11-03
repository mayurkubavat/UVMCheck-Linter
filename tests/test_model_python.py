#!/usr/bin/env python3
"""uvmcheck.model (Python over the C ABI) reads the same design as dv-model."""
import argparse
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "python"))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--lib", help="path to libuvmcheck_model")
    a = ap.parse_args()
    if a.lib:
        import os
        os.environ["UVMCHECK_MODEL_LIB"] = a.lib
    from uvmcheck.model import Design, ModelError

    failures = []

    def check(cond, what):
        if not cond:
            failures.append(what)

    d = Design.load([f"+incdir+{REPO / 'tests/common'}", str(REPO / "tests/map/map_tb.sv")])
    check(d.compile_errors == 0, "map_tb compiles")
    drv = d.find_class("my_drv")
    check(drv is not None and drv.uvm == "driver", "my_drv is a driver")
    base = d.classes[drv.base] if drv and drv.base is not None else None
    check(base is not None and base.name == "uvm_driver" and base.is_parameterized,
          "my_drv's base is the parameterized uvm_driver")
    check(d.derives_from(drv, "uvm_component"), "my_drv derives from uvm_component")
    test = d.find_class("my_test")
    env = [t for _, t in d.child_components(test)] if test else []
    check([t.name for t in env] == ["my_env"], "my_test holds my_env")
    check(d.file_name(drv.loc).endswith("map_tb.sv") and drv.loc.line == 7, "my_drv location")
    check(any(m.phase != "none" for c in d.classes for m in c.methods), "phase methods are marked")

    try:
        Design.load(["/no/such/file.sv"])
        check(False, "a missing file is an error")
    except ModelError as e:
        check("/no/such/file.sv" in str(e), "the error names the file")

    for f in failures:
        print(f"FAIL: {f}")
    print(f"test_model_python: {'ok' if not failures else f'{len(failures)} failed'}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
