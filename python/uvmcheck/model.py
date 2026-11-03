"""The uvmcheck design model from Python, through libuvmcheck_model's C ABI.

    from uvmcheck.model import Design
    d = Design.load(["+incdir+src", "src/tb_pkg.sv"])
    for c in d.classes:
        if c.uvm == "driver":
            print(c.name, d.file_name(c.loc), c.loc.line)

Same data as the Rust crate dv-model and the C++ rules (uvmcheck::model).
The library is found through $UVMCHECK_MODEL_LIB, then <repo>/build-model
(cmake --preset model). Standard library only.
"""

from __future__ import annotations

import ctypes
import os
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterator, List, Optional, Sequence

ABI_VERSION = 4
NONE = 0xFFFFFFFF

UVM_KINDS = ("none", "object", "sequence_item", "sequence", "component", "test", "env", "agent",
             "driver", "monitor", "sequencer", "scoreboard", "subscriber")
VISIBILITY = ("public", "protected", "local")
PHASES = ("none", "function", "task")
DIRECTIONS = ("in", "out", "inout", "ref")
SHAPES = ("other", "dynamic_array", "queue", "associative_array", "fixed_unpacked_array", "unpacked_struct")


class _Loc(ctypes.Structure):
    _fields_ = [("file", ctypes.c_uint32), ("line", ctypes.c_uint32), ("column", ctypes.c_uint32)]


class _Class(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("scope", ctypes.c_char_p), ("base_name", ctypes.c_char_p),
                ("base", ctypes.c_uint32), ("uvm", ctypes.c_uint8), ("is_virtual", ctypes.c_uint8),
                ("is_parameterized", ctypes.c_uint8), ("is_library", ctypes.c_uint8), ("loc", _Loc),
                ("property_count", ctypes.c_uint32), ("method_count", ctypes.c_uint32),
                ("constraint_count", ctypes.c_uint32)]


class _Property(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("type", ctypes.c_char_p), ("type_class", ctypes.c_uint32),
                ("visibility", ctypes.c_uint8), ("is_static", ctypes.c_uint8), ("is_const", ctypes.c_uint8),
                ("is_rand", ctypes.c_uint8), ("is_virtual_interface", ctypes.c_uint8), ("loc", _Loc)]


class _Method(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("visibility", ctypes.c_uint8), ("is_task", ctypes.c_uint8),
                ("is_virtual", ctypes.c_uint8), ("is_pure", ctypes.c_uint8), ("is_static", ctypes.c_uint8),
                ("phase", ctypes.c_uint8), ("declared_virtual", ctypes.c_uint8), ("is_builtin", ctypes.c_uint8),
                ("is_extern", ctypes.c_uint8), ("returns_void", ctypes.c_uint8), ("return_type", ctypes.c_char_p),
                ("loc", _Loc), ("argument_count", ctypes.c_uint32), ("local_count", ctypes.c_uint32),
                ("call_count", ctypes.c_uint32)]


class _Argument(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("type", ctypes.c_char_p), ("canonical_type", ctypes.c_char_p),
                ("type_class", ctypes.c_uint32), ("direction", ctypes.c_uint8), ("shape", ctypes.c_uint8),
                ("loc", _Loc)]


class _Local(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("type", ctypes.c_char_p), ("is_virtual_interface", ctypes.c_uint8),
                ("loc", _Loc)]


class _Call(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("callee_scope", ctypes.c_char_p), ("receiver", ctypes.c_char_p),
                ("loc", _Loc), ("first_arg_loc", _Loc), ("is_system", ctypes.c_uint8),
                ("callee_declared_virtual", ctypes.c_uint8), ("callee_builtin", ctypes.c_uint8),
                ("first_arg_is_null", ctypes.c_uint8), ("via_super", ctypes.c_uint8)]


class _Constraint(ctypes.Structure):
    _fields_ = [("name", ctypes.c_char_p), ("loc", _Loc)]


class ModelError(RuntimeError):
    pass


_lib = None


def _library_path() -> Path:
    env = os.environ.get("UVMCHECK_MODEL_LIB")
    if env:
        return Path(env)
    build = Path(__file__).resolve().parents[2] / "build-model"
    for name in ("libuvmcheck_model.so", "libuvmcheck_model.dylib", "uvmcheck_model.dll"):
        if (build / name).exists():
            return build / name
    raise ModelError(f"libuvmcheck_model not found in {build}; build it with "
                     "'cmake --preset model && cmake --build build-model' or set UVMCHECK_MODEL_LIB")


def _load_library():
    global _lib
    if _lib is not None:
        return _lib
    lib = ctypes.CDLL(str(_library_path()))
    p = ctypes.c_void_p
    sigs = {
        "uvmcheck_model_abi_version": ([], ctypes.c_uint32),
        "uvmcheck_design_load": ([ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)], p),
        "uvmcheck_design_free": ([p], None),
        "uvmcheck_design_error": ([p], ctypes.c_char_p),
        "uvmcheck_design_compile_errors": ([p], ctypes.c_uint32),
        "uvmcheck_design_time_ms": ([p, ctypes.c_int], ctypes.c_double),
        "uvmcheck_file_count": ([p], ctypes.c_uint32),
        "uvmcheck_file_name": ([p, ctypes.c_uint32], ctypes.c_char_p),
        "uvmcheck_class_count": ([p], ctypes.c_uint32),
        "uvmcheck_get_class": ([p, ctypes.c_uint32, ctypes.POINTER(_Class)], ctypes.c_int),
        "uvmcheck_get_property": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.POINTER(_Property)], ctypes.c_int),
        "uvmcheck_get_method": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.POINTER(_Method)], ctypes.c_int),
        "uvmcheck_get_argument": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32,
                                   ctypes.POINTER(_Argument)], ctypes.c_int),
        "uvmcheck_get_local": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32,
                                ctypes.POINTER(_Local)], ctypes.c_int),
        "uvmcheck_get_call": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_uint32,
                               ctypes.POINTER(_Call)], ctypes.c_int),
        "uvmcheck_get_constraint": ([p, ctypes.c_uint32, ctypes.c_uint32, ctypes.POINTER(_Constraint)], ctypes.c_int),
    }
    for name, (args, res) in sigs.items():
        fn = getattr(lib, name)
        fn.argtypes = args
        fn.restype = res
    if lib.uvmcheck_model_abi_version() != ABI_VERSION:
        raise ModelError(f"libuvmcheck_model ABI {lib.uvmcheck_model_abi_version()}, expected {ABI_VERSION}")
    _lib = lib
    return lib


def _s(b: Optional[bytes]) -> str:
    return b.decode("utf-8", "replace") if b else ""


def _id(v: int) -> Optional[int]:
    return None if v == NONE else v


@dataclass(frozen=True)
class Loc:
    file: int
    line: int
    column: int


@dataclass
class Property:
    name: str
    type: str
    type_class: Optional[int]
    visibility: str
    is_static: bool
    is_const: bool
    is_rand: bool
    is_virtual_interface: bool
    loc: Loc


@dataclass
class Argument:
    name: str
    type: str
    canonical_type: str
    type_class: Optional[int]
    direction: str
    shape: str
    loc: Loc


@dataclass
class Local:
    name: str
    type: str
    is_virtual_interface: bool
    loc: Loc


@dataclass
class Call:
    name: str              # as written: "randomize", "$urandom", "get"
    is_system: bool        # system task/function, or a built-in method such as randomize()
    callee_scope: str      # class (generic name) or package declaring the callee; "" if none
    callee_declared_virtual: bool
    callee_builtin: bool
    first_arg_is_null: bool
    first_arg_loc: Loc
    receiver: str          # object called on, as written ("m_env.ap"); "" if none
    via_super: bool        # written as super.name(...)
    loc: Loc


@dataclass
class Constraint:
    name: str
    loc: Loc


@dataclass
class Method:
    name: str
    visibility: str
    is_task: bool
    is_virtual: bool
    is_pure: bool
    is_static: bool
    phase: str
    declared_virtual: bool  # has the `virtual` keyword itself
    is_builtin: bool        # made by the compiler (randomize and the like)
    is_extern: bool         # declared by a prototype; loc is the body outside the class
    return_type: str
    returns_void: bool
    loc: Loc
    arguments: List[Argument] = field(default_factory=list)
    locals: List[Local] = field(default_factory=list)  # declared locals only
    calls: List[Call] = field(default_factory=list)    # in walk order, nested calls included


@dataclass
class Class:
    id: int
    name: str
    scope: str
    base_name: str
    base: Optional[int]
    uvm: str
    is_virtual: bool
    is_parameterized: bool
    is_library: bool
    loc: Loc
    properties: List[Property] = field(default_factory=list)
    methods: List[Method] = field(default_factory=list)
    constraints: List[Constraint] = field(default_factory=list)

    @property
    def is_component(self) -> bool:
        return UVM_KINDS.index(self.uvm) >= UVM_KINDS.index("component")


def _loc(l: _Loc) -> Loc:
    return Loc(l.file, l.line, l.column)


class Design:
    """An owned snapshot of a design; nothing refers back to the library."""

    def __init__(self, files: List[str], classes: List[Class], compile_errors: int, timings: dict):
        self.files = files
        self.classes = classes
        self.compile_errors = compile_errors
        self.timings = timings
        self._by_name = {}
        for c in classes:
            self._by_name.setdefault(c.name, c)

    @classmethod
    def load(cls, args: Sequence[str]) -> "Design":
        """Compile options as given to uvmcheck, e.g. ['-f', 'files.f']. Not thread safe."""
        lib = _load_library()
        argv = (ctypes.c_char_p * len(args))(*[a.encode() for a in args])
        handle = lib.uvmcheck_design_load(len(args), argv)
        try:
            err = lib.uvmcheck_design_error(handle)
            if err:
                raise ModelError(_s(err))
            start = time.perf_counter()
            files = [_s(lib.uvmcheck_file_name(handle, i)) for i in range(lib.uvmcheck_file_count(handle))]
            classes = []
            c, p, m = _Class(), _Property(), _Method()
            a, l, con, cl = _Argument(), _Local(), _Constraint(), _Call()
            for cid in range(lib.uvmcheck_class_count(handle)):
                if not lib.uvmcheck_get_class(handle, cid, ctypes.byref(c)):
                    raise ModelError(f"class {cid} missing")
                k = Class(cid, _s(c.name), _s(c.scope), _s(c.base_name), _id(c.base), UVM_KINDS[c.uvm],
                          bool(c.is_virtual), bool(c.is_parameterized), bool(c.is_library), _loc(c.loc))
                for i in range(c.property_count):
                    lib.uvmcheck_get_property(handle, cid, i, ctypes.byref(p))
                    k.properties.append(Property(_s(p.name), _s(p.type), _id(p.type_class),
                                                 VISIBILITY[p.visibility], bool(p.is_static),
                                                 bool(p.is_const), bool(p.is_rand),
                                                 bool(p.is_virtual_interface), _loc(p.loc)))
                for i in range(c.method_count):
                    lib.uvmcheck_get_method(handle, cid, i, ctypes.byref(m))
                    meth = Method(_s(m.name), VISIBILITY[m.visibility], bool(m.is_task),
                                  bool(m.is_virtual), bool(m.is_pure), bool(m.is_static),
                                  PHASES[m.phase], bool(m.declared_virtual), bool(m.is_builtin),
                                  bool(m.is_extern), _s(m.return_type), bool(m.returns_void), _loc(m.loc))
                    for j in range(m.argument_count):
                        lib.uvmcheck_get_argument(handle, cid, i, j, ctypes.byref(a))
                        meth.arguments.append(Argument(_s(a.name), _s(a.type), _s(a.canonical_type),
                                                       _id(a.type_class), DIRECTIONS[a.direction],
                                                       SHAPES[a.shape], _loc(a.loc)))
                    for j in range(m.local_count):
                        lib.uvmcheck_get_local(handle, cid, i, j, ctypes.byref(l))
                        meth.locals.append(Local(_s(l.name), _s(l.type), bool(l.is_virtual_interface),
                                                 _loc(l.loc)))
                    for j in range(m.call_count):
                        lib.uvmcheck_get_call(handle, cid, i, j, ctypes.byref(cl))
                        meth.calls.append(Call(_s(cl.name), bool(cl.is_system), _s(cl.callee_scope),
                                               bool(cl.callee_declared_virtual), bool(cl.callee_builtin),
                                               bool(cl.first_arg_is_null), _loc(cl.first_arg_loc),
                                               _s(cl.receiver), bool(cl.via_super), _loc(cl.loc)))
                    k.methods.append(meth)
                for i in range(c.constraint_count):
                    lib.uvmcheck_get_constraint(handle, cid, i, ctypes.byref(con))
                    k.constraints.append(Constraint(_s(con.name), _loc(con.loc)))
                classes.append(k)
            timings = {"parse_ms": lib.uvmcheck_design_time_ms(handle, 0),
                       "elaborate_ms": lib.uvmcheck_design_time_ms(handle, 1),
                       "model_ms": lib.uvmcheck_design_time_ms(handle, 2),
                       "copy_ms": (time.perf_counter() - start) * 1e3}
            return cls(files, classes, lib.uvmcheck_design_compile_errors(handle), timings)
        finally:
            lib.uvmcheck_design_free(handle)

    def find_class(self, name: str) -> Optional[Class]:
        return self._by_name.get(name)

    def file_name(self, loc: Loc) -> str:
        return self.files[loc.file] if 0 <= loc.file < len(self.files) else ""

    def ancestors(self, c: Class) -> Iterator[Class]:
        seen = {c.id}
        while c.base is not None and c.base not in seen:
            c = self.classes[c.base]
            seen.add(c.id)
            yield c

    def derives_from(self, c: Class, name: str) -> bool:
        return c.name == name or c.base_name == name or any(
            a.name == name or a.base_name == name for a in self.ancestors(c))

    def child_components(self, c: Class) -> Iterator[tuple]:
        for p in c.properties:
            if p.type_class is not None and self.classes[p.type_class].is_component:
                yield p, self.classes[p.type_class]
