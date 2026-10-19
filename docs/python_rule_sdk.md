# UVMCheck Python Plugin SDK

The **UVMCheck Python Plugin SDK** allows verification and CAD teams to author custom, company-specific SystemVerilog and UVM static lint checks in pure Python, without modifying or recompiling the core C++ engine.

---

## Key Benefits

- **Zero C++ Recompilation**: Write rules in Python and execute them immediately with `./build/uvmcheck --python-rules <path>`.
- **Full Reporting Pipeline Integration**: Violations emitted by Python rules participate identically in:
  - Terminal colorized diagnostics
  - SARIF 2.1.0 output for CI/CD code scanning (`--sarif`)
  - Standalone interactive HTML dashboard (`--html`)
  - Technical debt baselines (`--baseline` and `--update-baseline`)
  - External and inline comment waivers (`--waivers`)
- **Rich Semantic Model**: Inspect classes, inheritance trees, methods, tasks, properties, component paths, and UVM metadata.

---

## Getting Started

### 1. Installation
Install the SDK into your Python environment:
```bash
pip install -e python/
```
Or run directly by keeping the repository's `python/` folder in your path.

### 2. Creating a Custom Rule

Create a Python file, e.g. `rules/company_rules.py`:

```python
from uvmcheck import Rule, Severity, Context, ClassInfo

class CompanyDriverPrefixRule(Rule):
    """Enforces that all driver components begin with the company prefix 'cdrv_'."""
    id = "COMP-DRV-001"
    name = "CompanyDriverPrefix"
    category = "CustomStyle"
    description = "Driver components must start with prefix 'cdrv_' for corporate IP consistency."
    default_severity = Severity.Warning

    def analyze_class(self, cls: ClassInfo, context: Context) -> None:
        # Check if the class is derived from uvm_component and contains 'driver' in its name
        if context.is_uvm_component(cls) and "driver" in cls.name.lower():
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
```

### 3. Running Custom Rules

Run UVMCheck specifying your rule file or directory via `--python-rules`:

```bash
# Target a specific rule file
./build/uvmcheck tb/top.sv -I tb/include --python-rules rules/company_rules.py

# Target an entire directory of rules
./build/uvmcheck tb/top.sv -I tb/include --python-rules rules/

# Emit SARIF results containing both C++ built-in and Python custom violations
./build/uvmcheck tb/top.sv --python-rules rules/ --sarif report.sarif
```

---

## API Reference

### `Rule`
Base class for all custom rules.
- `id`: Unique string identifier (e.g. `COMP-XYZ-001`).
- `name`: Descriptive name (e.g. `CompanyNamingRule`).
- `category`: Functional category string.
- `description`: Detailed explanation of the check.
- `default_severity`: `Severity.Error`, `Severity.Warning`, or `Severity.Info`.
- `analyze(context)`: Main analysis entrypoint called on the entire design context.
- `analyze_class(cls, context)`: Hook called on each discovered class.

### `Context`
Provides query helpers and violation reporting:
- `classes`: List of all `ClassInfo` objects.
- `component_paths`: List of all hierarchical component paths.
- `is_uvm_component(cls)`: Returns `True` if class derives from `uvm_component`.
- `is_uvm_object(cls)`: Returns `True` if class derives from `uvm_object`.
- `is_uvm_sequence(cls)`: Returns `True` if class derives from `uvm_sequence`.
- `report(rule_id, message, filename, line, column=1, severity=Severity.Warning, rule_name=None)`: Emits a diagnostic.

### `ClassInfo`
- `name`: Class identifier.
- `base_class`: Base class identifier.
- `is_component`: Boolean.
- `is_object`: Boolean.
- `is_sequence`: Boolean.
- `filename`: Absolute or relative source file path.
- `line`, `column`: Location coordinates.
- `members`: List of `MemberInfo` items.
- `get_methods()`: Returns all methods (tasks/functions).
- `get_properties()`: Returns all member properties.
- `has_member(name)`: Checks member existence.
