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

    d = Design.load([f"+incdir+{REPO / 'testdata/common'}", str(REPO / "testdata/map/map_tb.sv")])
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

    f = Design.load([f"+incdir+{REPO / 'testdata/common'}", str(REPO / "testdata/model/facts.sv")])
    item = f.find_class("facts_item")
    check(item is not None and [k.name for k in item.constraints] == ["addr_c"], "constraints")
    s = next((m for m in item.methods if m.name == "sum"), None) if item else None
    check(s is not None and s.is_extern and s.declared_virtual and s.return_type == "int"
          and s.loc.line == 22, "extern method sum, located at its body")
    check(s is not None and [(x.name, x.direction, x.shape) for x in s.arguments]
          == [("q", "ref", "queue"), ("scale", "in", "other")], "arguments")
    check(s is not None and [x.name for x in s.locals] == ["total"], "declared locals only")
    p = next((m for m in item.methods if m.name == "probe"), None) if item else None
    check(p is not None and p.returns_void and p.locals and p.locals[0].is_virtual_interface,
          "virtual interface local")
    check(p is not None and [x.name for x in p.calls] == ["randomize", "set", "$urandom"]
          and p.calls[1].first_arg_is_null and p.calls[1].callee_scope == "uvm_config_db", "calls")
    sub = f.find_class("facts_sub")
    ms = {m.name: m for m in sub.methods} if sub else {}
    check("new" in ms and ms["new"].calls_super_new and ms["new"].super_new_index == 0, "super.new")
    check("write" in ms and [(x.lhs, x.root_kind) for x in ms["write"].assignments]
          == [("t.addr", "argument"), ("count", "property")], "assignments")
    check("wait_a_bit" in ms and [t.line for t in ms["wait_a_bit"].timing_controls] == [45], "timing controls")

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
