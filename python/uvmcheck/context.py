"""
The design model Python rules see.

uvmcheck exports the user's classes (UVM library and stub classes excluded) with their
UVM role, base chain, properties and methods; each method's arguments and the calls it
makes; config_db set/get calls; TLM ports and connect() calls; and the per-rule options
from the project config. Every element carries filename/line/column so a rule can report
on it directly: ``self.report(ctx, "message", at=method)``.
"""

from dataclasses import dataclass, field
from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple

from .rule import Severity, Violation


@dataclass
class Located:
    filename: str = ""
    line: int = 1
    column: int = 1

    @property
    def location(self) -> Tuple[str, int, int]:
        return (self.filename, self.line, self.column)


@dataclass
class Argument:
    name: str
    type: str
    direction: str = "input"  # input, output, inout, ref


@dataclass
class CallInfo(Located):
    """A call made inside a method: name, receiver text ("" for none), declaring class."""
    name: str = ""
    receiver: str = ""
    declared_in: str = ""
    arguments: List[str] = field(default_factory=list)


@dataclass
class MethodInfo(Located):
    name: str = ""
    kind: str = "function"  # function or task
    is_virtual: bool = False
    is_static: bool = False
    visibility: str = "public"
    return_type: str = ""
    arguments: List[Argument] = field(default_factory=list)
    calls: List[CallInfo] = field(default_factory=list)

    @property
    def is_task(self) -> bool:
        return self.kind == "task"

    def calls_named(self, *names: str) -> List[CallInfo]:
        return [c for c in self.calls if c.name in names]


@dataclass
class PropertyInfo(Located):
    name: str = ""
    type: str = ""
    rand: Optional[str] = None  # "rand", "randc" or None
    is_static: bool = False
    visibility: str = "public"


@dataclass
class MemberInfo:
    """Version 1 view of a member (property or method), kept for existing rules."""
    name: str
    is_method: bool
    type: str
    line: int
    column: int


@dataclass
class ClassInfo(Located):
    name: str = ""
    base_class: str = ""
    bases: List[str] = field(default_factory=list)  # nearest first
    role: str = "class"  # driver, monitor, agent, env, test, sequence, item, ...
    uvm_base: str = ""
    factory_registered: bool = False
    is_component: bool = False
    is_object: bool = False
    is_sequence: bool = False
    is_sequence_item: bool = False
    is_virtual: bool = False
    properties: List[PropertyInfo] = field(default_factory=list)
    methods: List[MethodInfo] = field(default_factory=list)

    @property
    def members(self) -> List[MemberInfo]:
        out = [MemberInfo(p.name, False, p.type, p.line, p.column) for p in self.properties]
        out += [MemberInfo(m.name, True, "", m.line, m.column) for m in self.methods]
        return out

    def derives_from(self, name: str) -> bool:
        return name in self.bases

    def method(self, name: str) -> Optional[MethodInfo]:
        return next((m for m in self.methods if m.name == name), None)

    def has_member(self, name: str) -> bool:
        return any(m.name == name for m in self.members)

    def get_methods(self) -> List[MemberInfo]:
        return [m for m in self.members if m.is_method]

    def get_properties(self) -> List[MemberInfo]:
        return [m for m in self.members if not m.is_method]


@dataclass
class ConfigDbAccess(Located):
    op: str = "set"  # set or get
    type: str = ""
    in_class: str = ""
    inst_name: str = ""
    field_name: str = ""


@dataclass
class TlmPort(Located):
    component: str = ""
    name: str = ""
    type: str = ""
    is_pull: bool = False
    is_analysis: bool = False


@dataclass
class TlmConnection(Located):
    in_class: str = ""
    source: str = ""
    target: str = ""


def _located(d: Dict[str, Any]) -> Dict[str, Any]:
    return {"filename": d.get("filename", ""), "line": d.get("line", 1), "column": d.get("column", 1)}


def _method(d: Dict[str, Any]) -> MethodInfo:
    return MethodInfo(
        name=d.get("name", ""), kind=d.get("kind", "function"), is_virtual=d.get("is_virtual", False),
        is_static=d.get("is_static", False), visibility=d.get("visibility", "public"),
        return_type=d.get("return_type", ""),
        arguments=[Argument(a["name"], a.get("type", ""), a.get("direction", "input")) for a in d.get("arguments", [])],
        calls=[CallInfo(name=c.get("name", ""), receiver=c.get("receiver", ""), declared_in=c.get("declared_in", ""),
                        arguments=c.get("arguments", []), **_located(c)) for c in d.get("calls", [])],
        **_located(d))


def _class(d: Dict[str, Any]) -> ClassInfo:
    props = [PropertyInfo(name=p.get("name", ""), type=p.get("type", ""), rand=p.get("rand"),
                          is_static=p.get("is_static", False), visibility=p.get("visibility", "public"),
                          **_located(p)) for p in d.get("properties", [])]
    methods = [_method(m) for m in d.get("methods", [])]
    # Version 1 models only list members.
    for m in d.get("members", []):
        loc = {"filename": d.get("filename", ""), "line": m.get("line", 1), "column": m.get("column", 1)}
        if m.get("is_method"):
            methods.append(MethodInfo(name=m.get("name", ""), **loc))
        else:
            props.append(PropertyInfo(name=m.get("name", ""), type=m.get("type", ""), **loc))
    base = d.get("base_class", "")
    return ClassInfo(
        name=d.get("name", ""), base_class=base, bases=d.get("bases", [base] if base else []),
        role=d.get("role", "class"), uvm_base=d.get("uvm_base", ""),
        factory_registered=d.get("factory_registered", False), is_component=d.get("is_component", False),
        is_object=d.get("is_object", False), is_sequence=d.get("is_sequence", False),
        is_sequence_item=d.get("is_sequence_item", False), is_virtual=d.get("is_virtual", False),
        properties=props, methods=methods, **_located(d))


class Context:
    """Query and reporting interface handed to every Python rule."""

    def __init__(self, raw_data: Dict[str, Any]):
        self.version: int = raw_data.get("version", 1)
        self.classes: List[ClassInfo] = [_class(c) for c in raw_data.get("classes", [])]
        self.config_db: List[ConfigDbAccess] = [
            ConfigDbAccess(op=c.get("op", "set"), type=c.get("type", ""), in_class=c.get("in_class", ""),
                           inst_name=c.get("inst_name", ""), field_name=c.get("field_name", ""), **_located(c))
            for c in raw_data.get("config_db", [])]
        self.tlm_ports: List[TlmPort] = [
            TlmPort(component=p.get("component", ""), name=p.get("name", ""), type=p.get("type", ""),
                    is_pull=p.get("is_pull", False), is_analysis=p.get("is_analysis", False), **_located(p))
            for p in raw_data.get("tlm_ports", [])]
        self.tlm_connections: List[TlmConnection] = [
            TlmConnection(in_class=c.get("in_class", ""), source=c.get("source", ""), target=c.get("target", ""),
                          **_located(c))
            for c in raw_data.get("tlm_connections", [])]
        self.component_paths: List[str] = raw_data.get("component_paths", [])
        self.rule_options: Dict[str, Dict[str, Any]] = raw_data.get("rule_options", {})
        self.violations: List[Violation] = []
        self._by_name = {c.name: c for c in self.classes}

    # ---- queries -------------------------------------------------------------------

    def find_class(self, name: str) -> Optional[ClassInfo]:
        return self._by_name.get(name)

    def classes_with_role(self, *roles: str) -> List[ClassInfo]:
        return [c for c in self.classes if c.role in roles]

    def subclasses_of(self, name: str) -> List[ClassInfo]:
        return [c for c in self.classes if c.derives_from(name)]

    def methods(self) -> Iterator[Tuple[ClassInfo, MethodInfo]]:
        for c in self.classes:
            for m in c.methods:
                yield c, m

    def calls(self, *names: str) -> Iterator[Tuple[ClassInfo, MethodInfo, CallInfo]]:
        """Every call (optionally only those with one of the given names), with its class and method."""
        for c, m in self.methods():
            for call in m.calls:
                if not names or call.name in names:
                    yield c, m, call

    def config_sets(self, field_name: Optional[str] = None) -> List[ConfigDbAccess]:
        return [a for a in self.config_db if a.op == "set" and (field_name is None or a.field_name == field_name)]

    def config_gets(self, field_name: Optional[str] = None) -> List[ConfigDbAccess]:
        return [a for a in self.config_db if a.op == "get" and (field_name is None or a.field_name == field_name)]

    def option(self, rule_id: str, key: str, default: Any = None, rule_name: Optional[str] = None) -> Any:
        """A rule option from the project config (RuleConfig attribute or JSON "options")."""
        for k in (rule_id, rule_name):
            if k and key in self.rule_options.get(k, {}):
                return self.rule_options[k][key]
        return default

    def is_uvm_component(self, cls: ClassInfo) -> bool:
        return cls.is_component

    def is_uvm_object(self, cls: ClassInfo) -> bool:
        return cls.is_object

    def is_uvm_sequence(self, cls: ClassInfo) -> bool:
        return cls.is_sequence

    # ---- reporting -----------------------------------------------------------------

    def report(
        self,
        rule_id: str,
        message: str,
        filename: str,
        line: int,
        column: int = 1,
        severity: Severity = Severity.Warning,
        rule_name: Optional[str] = None,
    ) -> None:
        """Report a lint violation discovered by a Python rule."""
        self.violations.append(Violation(
            rule_id=rule_id,
            rule_name=rule_name or rule_id,
            message=message,
            severity=severity,
            filename=filename,
            line=line,
            column=column,
        ))
