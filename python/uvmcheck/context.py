from dataclasses import dataclass, field
from typing import List, Optional, Dict, Any
from .rule import Severity, Violation

@dataclass
class MemberInfo:
    name: str
    is_method: bool
    type: str
    line: int
    column: int

@dataclass
class ClassInfo:
    name: str
    base_class: str
    is_component: bool
    is_object: bool
    is_sequence: bool
    is_sequence_item: bool
    filename: str
    line: int
    column: int
    members: List[MemberInfo] = field(default_factory=list)

    def has_member(self, name: str) -> bool:
        return any(m.name == name for m in self.members)

    def get_methods(self) -> List[MemberInfo]:
        return [m for m in self.members if m.is_method]

    def get_properties(self) -> List[MemberInfo]:
        return [m for m in self.members if not m.is_method]

class Context:
    """Provides querying and violation reporting capabilities to Python rules."""

    def __init__(self, raw_data: Dict[str, Any]):
        self.classes: List[ClassInfo] = []
        self.component_paths: List[str] = raw_data.get("component_paths", [])
        self.violations: List[Violation] = []

        for c in raw_data.get("classes", []):
            members = [
                MemberInfo(
                    name=m.get("name", ""),
                    is_method=m.get("is_method", False),
                    type=m.get("type", ""),
                    line=m.get("line", 1),
                    column=m.get("column", 1)
                )
                for m in c.get("members", [])
            ]
            self.classes.append(ClassInfo(
                name=c.get("name", ""),
                base_class=c.get("base_class", ""),
                is_component=c.get("is_component", False),
                is_object=c.get("is_object", False),
                is_sequence=c.get("is_sequence", False),
                is_sequence_item=c.get("is_sequence_item", False),
                filename=c.get("filename", ""),
                line=c.get("line", 1),
                column=c.get("column", 1),
                members=members
            ))

    def report(
        self,
        rule_id: str,
        message: str,
        filename: str,
        line: int,
        column: int = 1,
        severity: Severity = Severity.Warning,
        rule_name: Optional[str] = None
    ) -> None:
        """Report a lint violation discovered by a Python rule."""
        self.violations.append(Violation(
            rule_id=rule_id,
            rule_name=rule_name or rule_id,
            message=message,
            severity=severity,
            filename=filename,
            line=line,
            column=column
        ))

    def is_uvm_component(self, cls: ClassInfo) -> bool:
        return cls.is_component

    def is_uvm_object(self, cls: ClassInfo) -> bool:
        return cls.is_object

    def is_uvm_sequence(self, cls: ClassInfo) -> bool:
        return cls.is_sequence
