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
- **Rich Semantic Model**: Classes with their UVM role and base chain, properties, methods with arguments and the calls they make, config_db set/get calls, TLM ports and connections, and per-rule options from the project config.

---

## Getting Started

### 1. Installation
Install the SDK into your Python environment:
```bash
pip install -e python/
```
Or run directly by keeping the repository's `python/` folder in your path.

### 2. Creating a Custom Rule

Create a Python file, e.g. `rules/company_rules.py`. A rule overrides only the hooks it needs:

```python
from uvmcheck import Rule, Severity


class DriverPrefix(Rule):
    """Drivers start with the company prefix."""
    id = "COMP-DRV-001"
    name = "DriverPrefix"
    category = "CustomStyle"
    description = "Driver components must start with 'cdrv_'."
    default_severity = Severity.Warning

    def analyze_class(self, cls, ctx):
        prefix = self.option(ctx, "prefix", "cdrv_")
        if cls.role == "driver" and not cls.name.startswith(prefix):
            self.report(ctx, f"Driver '{cls.name}' must begin with '{prefix}'.", at=cls)


class NoGlobalOverrides(Rule):
    """Global type overrides only in tests."""
    id = "COMP-FAC-001"
    name = "NoGlobalOverrides"
    calls = {"set_type_override", "set_type_override_by_type"}   # on_call only sees these

    def on_call(self, cls, method, call, ctx):
        if cls.role != "test":
            self.report(ctx, f"{call.name}() in {cls.role} '{cls.name}'", at=call)


class UnreadConfigKey(Rule):
    """Whole-design checks override analyze()."""
    id = "COMP-CFG-001"
    name = "UnreadConfigKey"

    def analyze(self, ctx):
        read = {g.field_name for g in ctx.config_gets()}
        for s in ctx.config_sets():
            if s.field_name not in read:
                self.report(ctx, f"config_db key '{s.field_name}' is set but never read", at=s)
```

**Customizing a rule.** Rules are plain classes. To change one, subclass it in your own
file and override what differs; when a rule and its subclass are both loaded, only the
subclass runs. Mark a rule meant purely as a base with `abstract = True`.

**Options.** `self.option(ctx, key, default)` reads the rule's settings from the project
config, keyed by rule ID or by the rule's class name:

```python
# .uvmcheck/config.py
class Project(ProjectConfig):
    class DriverPrefix(RuleConfig):
        rule = "COMP-DRV-001"
        prefix = "acme_"
```

### 3. Running Custom Rules

```bash
uvmcheck tb/top.sv -I tb/include --python-rules rules/company_rules.py   # one file
uvmcheck tb/top.sv -I tb/include --python-rules rules/                    # a directory
```

Python findings take part in waivers, baselines, JSON, SARIF and HTML like built-in ones.
Anything a rule prints goes to stderr; if a rule raises, its traceback is shown and the
other rules' findings are still reported.

### 4. Testing Custom Rules

Use the same layout as the built-in rule tests, one folder per rule ID:

```
rule_tests/COMP-FAC-001/bad.sv        must produce exactly the findings in expect.json
rule_tests/COMP-FAC-001/good.sv       must produce none
rule_tests/COMP-FAC-001/expect.json   {"bad.sv": [{"rule": "COMP-FAC-001", "line": 17}]}
```

```bash
python3 -m uvmcheck.testing --rules rules/ --tests rule_tests/ --binary uvmcheck -- -I tb/include
```

---

## API Reference

### `Rule`
- Metadata: `id`, `name`, `category`, `description`, `default_severity`.
- `analyze(ctx)`: entry point. The default calls `analyze_class` for every class, then
  `on_method` for each method and `on_call` for each call (filtered by `calls`).
- `analyze_class(cls, ctx)`, `on_method(cls, method, ctx)`, `on_call(cls, method, call, ctx)`: hooks.
- `calls`: set of call names `on_call` receives (`None` = all).
- `option(ctx, key, default)`: rule option from the project config.
- `report(ctx, message, at=element, severity=None)`: report at any model element.

### `Context`
- `classes`: user classes (UVM library and stub excluded), as `ClassInfo`.
- `config_db`: `ConfigDbAccess` items (`op` "set"/"get", `type`, `in_class`, `inst_name`, `field_name`).
- `tlm_ports`: `TlmPort` (`component`, `name`, `type`, `is_pull`, `is_analysis`).
- `tlm_connections`: `TlmConnection` (`in_class`, `source`, `target`).
- `component_paths`, `rule_options`.
- Queries: `find_class(name)`, `classes_with_role(*roles)`, `subclasses_of(name)`, `methods()`,
  `calls(*names)`, `config_sets(field)`, `config_gets(field)`, `option(rule_id, key, default)`.
- `report(rule_id, message, filename, line, column=1, severity=..., rule_name=None)`.

### `ClassInfo`
- `name`, `base_class`, `bases` (nearest first), `role` (driver, monitor, sequencer, agent,
  scoreboard, subscriber, env, test, sequence, item, config, object, component, class, ...),
  `uvm_base`, `factory_registered`, `is_component`, `is_object`, `is_sequence`,
  `is_sequence_item`, `is_virtual`.
- `properties`: `PropertyInfo` (`name`, `type`, `rand`, `is_static`, `visibility`).
- `methods`: `MethodInfo` (`name`, `kind` "task"/"function", `is_virtual`, `is_static`,
  `visibility`, `return_type`, `arguments`, `calls`; `calls_named(*names)`).
- `derives_from(name)`, `method(name)`; `members`, `get_methods()`, `get_properties()`,
  `has_member(name)` as before.

### `CallInfo`
- `name`, `receiver` (text before the dot, "" for none), `declared_in` (class declaring the
  called method, e.g. `uvm_config_db`), `arguments` (source text of each argument).

Every element has `filename`, `line` and `column`.
