"""Rules used by tests/test_python_rules.py to exercise the Python rule API."""

from uvmcheck import Rule, Severity


class OverrideOutsideTest(Rule):
    """Uses on_call with a call filter and reports at the call."""
    id = "PY-OVR-001"
    name = "OverrideOutsideTest"
    calls = {"set_type_override", "set_type_override_by_type"}

    def on_call(self, cls, method, call, ctx):
        if cls.role != "test":
            self.report(ctx, f"{call.name}() in {cls.role} '{cls.name}'.{method.name}", at=call)


class StrictOverrideOutsideTest(OverrideOutsideTest):
    """Customizes the rule above by subclassing it: the base no longer runs on its own."""
    severity_override = Severity.Error

    def report(self, ctx, message, at=None, severity=None, **kw):
        super().report(ctx, "strict: " + message, at=at, severity=self.severity_override, **kw)


class UnreadConfigKey(Rule):
    """Uses config_db data, class lookups and a rule option from the project config."""
    id = "PY-CFG-001"
    name = "UnreadConfigKey"

    def analyze(self, ctx):
        ignore = set(self.option(ctx, "ignore", "").split(","))
        read = {g.field_name for g in ctx.config_gets()}
        for s in ctx.config_sets():
            if s.field_name not in read and s.field_name not in ignore:
                self.report(ctx, f"config_db key '{s.field_name}' is set but never read", at=s)
