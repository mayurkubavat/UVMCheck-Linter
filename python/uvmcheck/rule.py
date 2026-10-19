from enum import Enum
from dataclasses import dataclass
from typing import Optional, Any

class Severity(Enum):
    Error = "Error"
    Warning = "Warning"
    Info = "Info"

@dataclass
class Violation:
    rule_id: str
    rule_name: str
    message: str
    severity: Severity
    filename: str
    line: int
    column: int

class Rule:
    """Base class for all custom Python UVMCheck rules."""
    id: str = "CUSTOM-001"
    name: str = "CustomRule"
    category: str = "Custom"
    description: str = "User-authored custom verification rule."
    default_severity: Severity = Severity.Warning

    def analyze(self, context: Any) -> None:
        """
        Main analysis entrypoint called on the entire design context.
        Override to perform global cross-component analysis.
        By default, delegates to analyze_class for every discovered class.
        """
        for cls in context.classes:
            self.analyze_class(cls, context)

    def analyze_class(self, cls: Any, context: Any) -> None:
        """
        Invoked for each class discovered in the SystemVerilog codebase.
        Override to inspect individual classes, properties, or methods.
        """
        pass
