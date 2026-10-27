# Project Configuration

A project config tells UVMCheck which code is yours and how each rule should
behave. It is written in Python as ordinary classes, so a team can keep a
shared base config and each block can override just what it needs.

```bash
# Write a starter config, then edit it
./build/uvmcheck --dump-config .uvmcheck/config.py
```

UVMCheck looks for configuration files in the working directory and its parents (first hit wins):
1. `.uvmcheck/config.py`
2. `.uvmcheck/config.json`
3. `uvmcheck_config.py`
4. `.uvmcheck.py`
5. `.uvmcheck.json`
6. `uvmcheck.json`

You can also specify `--config <file>` explicitly. A config that fails to load stops the run with exit
code 2 instead of silently linting with defaults.

In addition, `.uvmcheck/waivers.json` and `.uvmcheck/baseline.json` are automatically discovered if present.

## Example

```python
from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity


class Project(ProjectConfig):
    # Third-party code: compiled so your testbench elaborates, never reported.
    library_paths = ["$UVM_HOME", "vip/**"]

    # Your own files that should not be reported (generated code, etc.).
    exclude_paths = ["**/generated/**"]

    # One class per rule. The class name is the rule name, or set `rule`.
    class MaxLineLength(RuleConfig):
        max_columns = 140                    # rule option

    class NoCasex(RuleConfig):
        rule = "SV-COR-009"
        severity = Severity.Error
        exclude_paths = ["legacy/**"]       # this rule only

    class GenericReportId(RuleConfig):
        enabled = False

    waivers = [
        Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142,
               reason="Objection in loop verified for stress bench"),
        LegacyMacros,
    ]


# A reusable waiver is a class; subclass it to widen or narrow it.
class LegacyMacros(Waiver):
    rules = ["SV-STY-002", "OneStatementPerLine"]
    paths = ["**/legacy/**"]
    reason = "Vendor macros we cannot edit"
    owner = "dv-infra"
    expires = "2027-06-30"     # stops applying after this date, with a warning
```

(Define `LegacyMacros` above `Project` in a real file; it is shown after for reading order.)

## Sharing and overriding

Configs can import a base class from a file next to them (the config's folder
is on the import path):

```python
# team_base.py
class CompanyBase(ProjectConfig):
    library_paths = ["$UVM_HOME", "$VIP_ROOT"]

    class MaxLineLength(RuleConfig):
        max_columns = 120

# uvmcheck_config.py
from team_base import CompanyBase

class DmaBlock(CompanyBase):
    library_paths = CompanyBase.library_paths + ["third_party/axi_vip"]

    class MaxLineLength(CompanyBase.MaxLineLength):   # override one option
        max_columns = 160

    waivers = CompanyBase.waivers + [
        Waiver("SV-STY-002", path="**/legacy/**", reason="Vendor macros"),
    ]
```

Rule classes are inherited; defining one with the same rule key replaces the
parent's. If a file defines several unrelated `ProjectConfig` classes, pick one
with `config = DmaBlock`. For anything dynamic, override `rule_configs()` or
`to_dict()`.

## Reference

| Setting | Meaning |
| :--- | :--- |
| `library_paths` | Globs for third-party code (UVM, VIP). Files are still compiled; violations in them are dropped. |
| `exclude_paths` | Globs for your own files whose violations should be dropped. |
| `report_library_macro_expansions` | Default `False`: code produced by a library macro (`` `uvm_object_utils `` etc.) inside your file counts as library code. |
| `waivers` | `Waiver(...)` objects or `Waiver` subclasses with `rules`, `paths`, `line`, `reason` (mandatory), `owner`, `expires` (YYYY-MM-DD). Empty `rules` / `paths` mean every rule / file. |
| `style`, `naming` | Same dicts as the legacy JSON `style` / `naming` blocks. |
| `RuleConfig.rule` | Rule ID (`"SV-STY-016"`) or name (`"MaxLineLength"`); defaults to the class name. |
| `RuleConfig.enabled` | `False` skips the rule entirely (also for Python plugin rules). |
| `RuleConfig.severity` | `Severity.Error`, `Severity.Warning` or `Severity.Info`. |
| `RuleConfig.exclude_paths` | Globs where only this rule is silenced. |
| any other attribute | A rule option, e.g. `max_columns` (SV-STY-016), `max_complexity` / `warning_complexity` (SV-SML-004). |

Unknown rule names produce a warning so typos do not go unnoticed.

### Path globs

* Relative paths are relative to the config file; `--library` / `--exclude`
  on the command line are relative to the working directory.
* `*` and `?` match within one folder name, `**` matches any number of folders.
* A pattern naming a folder covers everything under it (`"ip/vip"` = `"ip/vip/**"`).
* A pattern with no `/` matches that name at any depth (`"generated"`, `"*_pkg.sv"`).
* `$VAR`, `${VAR}` and a leading `~` are expanded. A pattern whose variable is
  unset is ignored with a warning, so `$UVM_HOME/**` can never turn into `/**`.

### Command line

```bash
./build/uvmcheck tb/top.sv -I $UVM_HOME/src --library '$UVM_HOME' --exclude 'tb/gen/**'
```

### JSON form

The same settings are accepted as JSON (`.uvmcheck.json`), which is also what
the Python config is converted to internally. Comments and trailing commas are
allowed. Rules may use a shorthand: `"SV-COR-009": "error"`, `"MaxLineLength": "off"`.

```json
{
  "library_paths": ["$UVM_HOME", "vip/**"],
  "rules": {
    "MaxLineLength": { "options": { "max_columns": 140 } },
    "SV-COR-009": { "severity": "ERROR", "exclude_paths": ["legacy/**"] },
    "GenericReportId": "off"
  },
  "waivers": [
    { "rule": "UVM-OBJ-002", "path": "tb/stress_seq.sv", "line": 142, "reason": "Stress bench" }
  ]
}
```

### Environment

* `UVMCHECK_PYTHON` selects the interpreter (default `python3`).
* `UVMCHECK_PYTHON_SDK` points at the folder holding the `uvmcheck` package if
  it is not installed and not next to the binary.
