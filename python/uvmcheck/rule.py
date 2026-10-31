"""
Base class for Python rules.

Subclass ``Rule``, set its metadata, and override the hooks you need. The runner calls
``analyze(ctx)``, which by default walks the model and calls, in order:

    analyze_class(cls, ctx)            every user class
    on_method(cls, method, ctx)        every method of that class
    on_call(cls, method, call, ctx)    every call in that method; set ``calls = {...}``
                                       to receive only calls with those names

Report with ``self.report(ctx, "message", at=element)``; any model element (class,
method, call, property, config_db access, port) gives the location. Rule options come
from the project config: ``self.option(ctx, "max_depth", 3)`` reads
``class MyRule(RuleConfig): max_depth = 5`` (or the JSON "options" of the rule).

Rules are ordinary classes, so a team customizes one by subclassing it and overriding a
method or attribute; only leaf classes run (a rule with subclasses in the same files is
treated as a base class).
"""

from dataclasses import dataclass
from enum import Enum
from typing import Any, ClassVar, Optional, Set


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
    # Names of calls on_call() receives; None means every call.
    calls: ClassVar[Optional[Set[str]]] = None
    # Set True on a rule meant only as a base for other rules.
    abstract: ClassVar[bool] = False

    def analyze(self, context: Any) -> None:
        """Entry point. The default walks classes, methods and calls (see module docstring)."""
        wants_methods = type(self).on_method is not Rule.on_method
        wants_calls = type(self).on_call is not Rule.on_call
        for cls in context.classes:
            self.analyze_class(cls, context)
            if not (wants_methods or wants_calls):
                continue
            for method in cls.methods:
                if wants_methods:
                    self.on_method(cls, method, context)
                if wants_calls:
                    for call in method.calls:
                        if self.calls is None or call.name in self.calls:
                            self.on_call(cls, method, call, context)

    def analyze_class(self, cls: Any, context: Any) -> None:
        """Called for each user class."""

    def on_method(self, cls: Any, method: Any, context: Any) -> None:
        """Called for each method of each user class."""

    def on_call(self, cls: Any, method: Any, call: Any, context: Any) -> None:
        """Called for each call (filtered by ``calls``) in each method."""

    def option(self, context: Any, key: str, default: Any = None) -> Any:
        return context.option(self.id, key, default, rule_name=type(self).__name__)

    def report(self, context: Any, message: str, at: Any = None, severity: Optional[Severity] = None,
               filename: str = "", line: int = 1, column: int = 1) -> None:
        """Report a finding at a model element (``at``) or an explicit location."""
        if at is not None:
            filename, line, column = at.filename, at.line, at.column
        context.report(rule_id=self.id, rule_name=self.name, message=message,
                       severity=severity or self.default_severity,
                       filename=filename, line=line, column=column)
