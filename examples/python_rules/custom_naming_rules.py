"""
Example Custom Python Verification Rules for UVMCheck
Demonstrates how verification teams can author proprietary checks in pure Python.
"""

from uvmcheck import Rule, Severity, Context, ClassInfo

class CompanyDriverPrefixRule(Rule):
    """Enforces that all driver components begin with the company prefix 'cdrv_'."""
    id = "COMP-DRV-001"
    name = "CompanyDriverPrefix"
    category = "CustomStyle"
    description = "Driver components must start with prefix 'cdrv_' for corporate IP consistency."
    default_severity = Severity.Warning

    def analyze_class(self, cls: ClassInfo, context: Context) -> None:
        if context.is_uvm_component(cls) and ("driver" in cls.name.lower() or "drv" in cls.name.lower()):
            if not cls.name.startswith("cdrv_"):
                context.report(
                    rule_id=self.id,
                    rule_name=self.name,
                    message=f"Company policy violation: Driver '{cls.name}' must begin with prefix 'cdrv_'.",
                    severity=self.default_severity,
                    filename=cls.filename,
                    line=cls.line,
                    column=cls.column
                )

class CompanyScoreboardSuffixRule(Rule):
    """Enforces that all scoreboard components end with the suffix '_scb'."""
    id = "COMP-SCB-001"
    name = "CompanyScoreboardSuffix"
    category = "CustomStyle"
    description = "Scoreboard components must end with suffix '_scb'."
    default_severity = Severity.Warning

    def analyze_class(self, cls: ClassInfo, context: Context) -> None:
        if context.is_uvm_component(cls) and ("scoreboard" in cls.name.lower() or "scb" in cls.name.lower()):
            if not cls.name.endswith("_scb"):
                context.report(
                    rule_id=self.id,
                    rule_name=self.name,
                    message=f"Company policy violation: Scoreboard '{cls.name}' must end with suffix '_scb'.",
                    severity=self.default_severity,
                    filename=cls.filename,
                    line=cls.line,
                    column=cls.column
                )
