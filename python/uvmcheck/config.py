"""
UVMCheck project configuration, written as plain Python classes.

A project config is a ``ProjectConfig`` subclass. Rule settings are
``RuleConfig`` subclasses attached to it, and waivers are ``Waiver`` objects.
Because everything is a class, teams can share a base config and override
just the parts they need:

    from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity

    class CompanyBase(ProjectConfig):
        library_paths = ["$UVM_HOME", "vip/**"]

        class MaxLineLength(RuleConfig):
            max_columns = 120

    class MyBlock(CompanyBase):
        exclude_paths = ["**/generated/**"]

        class MaxLineLength(CompanyBase.MaxLineLength):
            max_columns = 140          # override one option, keep the rest

        waivers = CompanyBase.waivers + [
            Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142,
                   reason="Objection in loop verified for stress bench"),
        ]

    class LegacyMacros(Waiver):        # reusable, subclassable waiver
        rules = ["SV-STY-002"]
        paths = ["**/legacy/**"]
        reason = "Vendor macros we cannot edit"
        expires = "2027-06-30"

The C++ engine runs this file through ``python3 -m uvmcheck.config_runner``,
which serializes the selected config to JSON with ``to_dict()``.
"""

import datetime
import inspect
from typing import Any, Dict, List, Optional, Union

from .rule import Severity

__all__ = ["ProjectConfig", "RuleConfig", "Waiver", "Severity", "ConfigError"]


class ConfigError(Exception):
    """Raised for a config that is well-formed Python but not a valid UVMCheck config."""


def _severity_name(value: Union[Severity, str, None]) -> Optional[str]:
    if value is None:
        return None
    if isinstance(value, Severity):
        return value.value.upper()
    name = str(value).strip().upper()
    if name not in ("ERROR", "WARNING", "INFO"):
        raise ConfigError(f"unknown severity {value!r}; use Severity.Error, Severity.Warning or Severity.Info")
    return name


def _path_list(owner: str, attr: str, value: Any) -> List[str]:
    if isinstance(value, str):
        return [value]
    if not isinstance(value, (list, tuple)) or not all(isinstance(p, str) for p in value):
        raise ConfigError(f"{owner}.{attr} must be a list of path strings")
    return list(value)


class RuleConfig:
    """Settings for one rule.

    The rule is identified by ``rule`` (an ID such as ``"SV-STY-016"`` or a
    name such as ``"MaxLineLength"``); when ``rule`` is not set, the class name
    is used. Any other public class attribute becomes a rule option that the
    rule reads at run time (for example ``max_columns`` on MaxLineLength).
    """

    rule: Optional[str] = None
    enabled: bool = True
    severity: Optional[Severity] = None
    exclude_paths: List[str] = []

    _RESERVED = ("rule", "enabled", "severity", "exclude_paths")

    @classmethod
    def rule_key(cls) -> str:
        return cls.rule or cls.__name__

    @classmethod
    def options(cls) -> Dict[str, Any]:
        """Collect option attributes across the class hierarchy (subclasses win)."""
        opts: Dict[str, Any] = {}
        for klass in reversed(cls.__mro__):
            if klass is object or klass is RuleConfig:
                continue
            for key, value in vars(klass).items():
                if key.startswith("_") or key in RuleConfig._RESERVED:
                    continue
                if callable(value) or isinstance(value, (classmethod, staticmethod, property)):
                    continue
                if not isinstance(value, (bool, int, float, str)):
                    raise ConfigError(
                        f"{cls.__qualname__}.{key}: rule options must be bool, int, float or str")
                opts[key] = value
        return opts

    @classmethod
    def to_dict(cls) -> Dict[str, Any]:
        out: Dict[str, Any] = {"enabled": bool(cls.enabled)}
        sev = _severity_name(cls.severity)
        if sev:
            out["severity"] = sev
        paths = _path_list(cls.__qualname__, "exclude_paths", cls.exclude_paths)
        if paths:
            out["exclude_paths"] = paths
        opts = cls.options()
        if opts:
            out["options"] = opts
        return out


class Waiver:
    """An approved exception. ``reason`` is mandatory so every waiver is reviewable.

    Write one inline:

        Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142, reason="...")

    or as a class, which can be shared and subclassed:

        class LegacyMacros(Waiver):
            rules = ["SV-STY-002", "SV-STY-011"]
            paths = ["**/legacy/**"]
            reason = "Vendor macros we cannot edit"
            owner = "dv-infra"
            expires = "2027-06-30"

    ``rules`` / ``paths`` default to every rule / every file; ``line`` to any
    line. ``paths`` use the same glob syntax as ``library_paths``. A waiver
    past its ``expires`` date (YYYY-MM-DD) stops applying and is reported.
    """

    rules: List[str] = []
    paths: List[str] = []
    line: Optional[int] = None
    reason: str = ""
    owner: Optional[str] = None
    expires: Optional[str] = None

    def __init__(self, rule: Optional[str] = None, *, reason: Optional[str] = None,
                 path: Optional[str] = None, line: Optional[int] = None,
                 rules: Optional[List[str]] = None, paths: Optional[List[str]] = None,
                 owner: Optional[str] = None, expires: Optional[str] = None):
        if rule is not None:
            self.rules = [rule]
        if rules is not None:
            self.rules = list(rules)
        if path is not None:
            self.paths = [path]
        if paths is not None:
            self.paths = list(paths)
        if line is not None:
            self.line = line
        if reason is not None:
            self.reason = reason
        if owner is not None:
            self.owner = owner
        if expires is not None:
            self.expires = expires
        self.validate()

    @property
    def label(self) -> str:
        return type(self).__qualname__ if type(self) is not Waiver else f"Waiver({', '.join(self.rules) or '*'})"

    def validate(self) -> None:
        if isinstance(self.rules, str) or not all(isinstance(r, str) and r for r in self.rules):
            raise ConfigError(f"{self.label}: rules must be a list of rule IDs or names")
        _path_list(self.label, "paths", self.paths)
        if not self.reason or not str(self.reason).strip():
            raise ConfigError(f"{self.label} needs a non-empty reason")
        if self.line is not None and (not isinstance(self.line, int) or self.line < 1):
            raise ConfigError(f"{self.label}: line must be a positive integer")
        if self.expires is not None:
            try:
                datetime.date.fromisoformat(str(self.expires))
            except ValueError:
                raise ConfigError(f"{self.label}: expires must be a YYYY-MM-DD date, got {self.expires!r}")

    def is_expired(self, today: Optional[datetime.date] = None) -> bool:
        if self.expires is None:
            return False
        return datetime.date.fromisoformat(str(self.expires)) < (today or datetime.date.today())

    def to_dict(self) -> Dict[str, Any]:
        out: Dict[str, Any] = {"rules": list(self.rules) or ["*"], "reason": self.reason}
        if self.paths:
            out["paths"] = list(self.paths)
        if self.line:
            out["line"] = self.line
        if self.owner:
            out["owner"] = self.owner
        if self.expires:
            out["expires"] = str(self.expires)
        return out

    def __repr__(self) -> str:
        return f"{self.label}(paths={self.paths!r}, line={self.line!r})"


class ProjectConfig:
    """Base class for a project config.

    library_paths  Third-party code (UVM, VIP). Compiled so your code
                   elaborates, but violations inside it are not reported.
    exclude_paths  Your own files that should not be linted (generated code).
    report_library_macro_expansions
                   False (default): code expanded from a library macro, such as
                   `uvm_object_utils in your class, counts as library code.
    waivers        List of Waiver objects or Waiver subclasses.
    style, naming  Optional dicts with the same keys as the JSON config.

    Rule settings are RuleConfig subclasses defined in (or assigned to) the
    class body. Subclasses inherit them and may replace one by defining a
    RuleConfig with the same rule key. Override ``rule_configs()`` or
    ``to_dict()`` for anything more dynamic.
    """

    library_paths: List[str] = []
    exclude_paths: List[str] = []
    report_library_macro_expansions: bool = False
    waivers: List[Waiver] = []
    style: Dict[str, Any] = {}
    naming: Dict[str, Any] = {}

    def rule_configs(self) -> List[type]:
        """All RuleConfig classes visible on this config, most-derived wins per rule key."""
        by_key: Dict[str, type] = {}
        for klass in reversed(type(self).__mro__):
            for value in vars(klass).values():
                if inspect.isclass(value) and issubclass(value, RuleConfig) and value is not RuleConfig:
                    by_key[value.rule_key()] = value
        return list(by_key.values())

    def to_dict(self) -> Dict[str, Any]:
        name = type(self).__qualname__
        out: Dict[str, Any] = {
            "library_paths": _path_list(name, "library_paths", self.library_paths),
            "exclude_paths": _path_list(name, "exclude_paths", self.exclude_paths),
            "report_library_macro_expansions": bool(self.report_library_macro_expansions),
            "rules": {rc.rule_key(): rc.to_dict() for rc in self.rule_configs()},
        }
        out["waivers"] = []
        warnings: List[str] = []
        for w in self.waivers:
            if inspect.isclass(w) and issubclass(w, Waiver):
                w = w()
            if not isinstance(w, Waiver):
                raise ConfigError(f"{name}.waivers must contain Waiver objects or classes, got {w!r}")
            if w.is_expired():
                owner = f" (owner: {w.owner})" if w.owner else ""
                warnings.append(f"waiver {w.label} expired on {w.expires}{owner}; it no longer applies")
                continue
            out["waivers"].append(w.to_dict())
        if warnings:
            out["warnings"] = warnings
        if self.style:
            out["style"] = dict(self.style)
        if self.naming:
            out["naming"] = dict(self.naming)
        return out
