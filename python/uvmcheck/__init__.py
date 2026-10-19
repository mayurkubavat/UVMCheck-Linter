"""
UVMCheck Python Plugin SDK
Enables verification teams to author custom SystemVerilog and UVM lint rules in Python.
"""

from .rule import Rule, Severity, Violation
from .context import Context, ClassInfo, MemberInfo

__version__ = "0.1.0"
__all__ = [
    "Rule",
    "Severity",
    "Violation",
    "Context",
    "ClassInfo",
    "MemberInfo",
]
