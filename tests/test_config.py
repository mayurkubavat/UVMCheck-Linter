#!/usr/bin/env python3
"""
Integration tests for the UVMCheck project config (uvmcheck_config.py / .uvmcheck.json):
library and exclude paths, per-rule settings and options, waivers, and error handling.

Each case builds a small project in a temp directory with a "library" file and a
"user" file that both trigger the same rules, then checks what gets reported.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

LONG = "x" * 130
# Triggers SV-COR-001 (TautologicalCheckCompare), SV-COR-008 (SelfComparison)
# and SV-STY-016 (MaxLineLength).
BODY = textwrap.dedent(f"""\
    module {{name}};
      int a;
      initial begin
        if (a == a) $display("{LONG}");
      end
    endmodule : {{name}}
    """)

LIB_MACRO = textwrap.dedent("""\
    `define LIB_SELF_CHECK if (lib_v == lib_v) $display("same");
    """)

USER_WITH_MACRO = textwrap.dedent("""\
    `include "lib_macros.svh"
    module user_macro;
      int lib_v;
      initial begin
        `LIB_SELF_CHECK
      end
    endmodule : user_macro
    """)

VIOLATION_RE = re.compile(r"^(?P<file>[^:\s]+):(?P<line>\d+):(?P<col>\d+): \[(?P<sev>[A-Z]+)\] (?P<rule>[A-Z0-9.\-_]+) ")

failures = []


def make_project(root: Path) -> None:
    (root / "lib").mkdir()
    (root / "src").mkdir()
    (root / "lib" / "vendor_vip.sv").write_text(BODY.format(name="vendor_vip"))
    (root / "src" / "user_tb.sv").write_text(BODY.format(name="user_tb"))


def run(binary, cwd, args, env=None):
    full_env = dict(os.environ)
    full_env.pop("UVM_HOME", None)
    if env:
        full_env.update(env)
    res = subprocess.run([binary] + args, cwd=cwd, capture_output=True, text=True, env=full_env)
    out = re.sub(r"\x1b\[[0-9;]*m", "", res.stdout)
    err = re.sub(r"\x1b\[[0-9;]*m", "", res.stderr)
    found = []
    for line in out.splitlines():
        m = VIOLATION_RE.match(line)
        if m:
            found.append((os.path.basename(m["file"]), m["rule"], m["sev"]))
    return res.returncode, found, out, err


def check(name, cond, detail=""):
    if cond:
        print(f"  [PASS] {name}")
    else:
        print(f"  [FAIL] {name}")
        if detail:
            print(textwrap.indent(detail, "      "))
        failures.append(name)


def files_with(found, rule):
    return sorted({f for f, r, _ in found if r == rule})


def case(binary, title, config_name, config_text, args=None, env=None, extra=None):
    """Create a project, write the config, run the linter on both files."""
    tmp = tempfile.TemporaryDirectory()
    root = Path(tmp.name)
    make_project(root)
    if extra:
        for rel, text in extra.items():
            dest = root / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(text)
    if config_name:
        dest = root / config_name
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(textwrap.dedent(config_text))
    sources = args if args is not None else ["lib/vendor_vip.sv", "src/user_tb.sv"]
    rc, found, out, err = run(binary, root, sources, env)
    print(f"\n{title}")
    return tmp, rc, found, out, err


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = os.path.abspath(ap.parse_args().binary)

    # 1. Python config, library path suppresses vendor file only.
    t, rc, found, out, err = case(binary, "Python config: library_paths", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            library_paths = ["lib"]
        """)
    check("user file still reported", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)
    check("summary counts library suppressions", "in library paths" in out, out)

    # 2. Same thing in JSON.
    t, rc, found, out, err = case(binary, "JSON config: library_paths", ".uvmcheck.json",
                                  '{ "library_paths": ["lib/**"], // comments allowed\n }')
    check("json library path suppresses vendor file", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    # 3. Environment variables.
    t, rc, found, out, err = case(binary, "Env var in library path", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            library_paths = ["$VIP_HOME"]
        """, env={"VIP_HOME": "lib"})
    check("$VIP_HOME relative value resolves", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    # Library path spelled through a symlink (e.g. macOS /tmp -> /private/tmp).
    with tempfile.TemporaryDirectory() as alias_dir:
        alias = Path(alias_dir) / "alias"
        os.symlink(t.name, alias)
        (Path(t.name) / "uvmcheck_config.py").write_text(textwrap.dedent("""\
            from uvmcheck.config import ProjectConfig
            class Project(ProjectConfig):
                library_paths = ["$VIP_HOME"]
            """))
        rc, found, out, err = run(binary, t.name, ["lib/vendor_vip.sv", "src/user_tb.sv"],
                                  env={"VIP_HOME": str(alias / "lib")})
        print("\nLibrary path through a symlink")
        check("symlinked library path suppresses vendor file",
              files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    t, rc, found, out, err = case(binary, "Unset env var", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            library_paths = ["$UVM_HOME/**"]
        """)
    check("unset var warns", "unset environment variable" in err, err)
    check("unset var does not suppress everything",
          files_with(found, "SV-COR-008") == ["user_tb.sv", "vendor_vip.sv"], out)

    # 4. exclude_paths and per-rule exclude_paths.
    t, rc, found, out, err = case(binary, "exclude_paths + rule exclude_paths", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, RuleConfig
        class Project(ProjectConfig):
            exclude_paths = ["vendor_vip.sv"]
            class MaxLineLength(RuleConfig):
                exclude_paths = ["src/**"]
        """)
    check("exclude_paths drops vendor file", "vendor_vip.sv" not in {f for f, _, _ in found}, out)
    check("rule exclude drops only that rule", files_with(found, "SV-STY-016") == [] and
          files_with(found, "SV-COR-008") == ["user_tb.sv"], out)

    # 4b. only_paths keeps a rule to some files; other rules still report everywhere.
    t, rc, found, out, err = case(binary, "rule only_paths", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, RuleConfig
        class Project(ProjectConfig):
            class SelfComparison(RuleConfig):
                only_paths = ["lib/**"]
        """)
    check("only_paths keeps the rule to the listed files",
          files_with(found, "SV-COR-008") == ["vendor_vip.sv"], out + err)
    t, rc, found, out, err = case(binary, "JSON rule only_paths", ".uvmcheck.json",
                                  '{ "rules": { "SV-COR-008": { "only_paths": ["src/**"] } } }')
    check("JSON only_paths keeps the rule to the listed files",
          files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    # 5. Per-rule enable / severity / options, OOP inheritance from a shared base file.
    base = textwrap.dedent("""\
        from uvmcheck.config import ProjectConfig, RuleConfig, Severity
        class TeamBase(ProjectConfig):
            library_paths = ["lib"]
            class MaxLineLength(RuleConfig):
                max_columns = 100
            class SelfComparison(RuleConfig):
                severity = Severity.Info
        """)
    t, rc, found, out, err = case(binary, "Inheritance, overrides and options", "uvmcheck_config.py", """\
        from team_base import TeamBase
        from uvmcheck.config import RuleConfig
        class Block(TeamBase):
            class MaxLineLength(TeamBase.MaxLineLength):
                max_columns = 200
            class NoTautology(RuleConfig):
                rule = "SV-COR-001"
                enabled = False
        """, extra={"team_base.py": base})
    check("inherited library path applies", "vendor_vip.sv" not in {f for f, _, _ in found}, out + err)
    check("overridden option max_columns=200 silences SV-STY-016", files_with(found, "SV-STY-016") == [], out)
    check("inherited severity override (INFO)",
          [s for _, r, s in found if r == "SV-COR-008"] == ["INFO"], out)
    check("rule disabled by ID", files_with(found, "SV-COR-001") == [], out)

    t, rc, found, out, err = case(binary, "Rule disabled by name, JSON shorthand", ".uvmcheck.json",
                                  '{ "rules": { "TautologicalCheckCompare": "off", "SV-COR-008": "error" } }')
    check("control: other rules still fire", files_with(found, "SV-STY-016") == ["user_tb.sv", "vendor_vip.sv"], out)
    check("\"off\" disables by name", files_with(found, "SV-COR-001") == [], out)
    check("severity shorthand", {s for _, r, s in found if r == "SV-COR-008"} == {"ERROR"}, out)

    t, rc, found, out, err = case(binary, "Rule option lowers threshold", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, RuleConfig
        class Project(ProjectConfig):
            class MaxLineLength(RuleConfig):
                max_columns = 100
        """)
    check("max_columns=100 reported in message", "threshold of 100 characters" in out, out)

    # 6. Waivers.
    t, rc, found, out, err = case(binary, "Waivers", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, Waiver
        class Project(ProjectConfig):
            waivers = [Waiver("SV-COR-008", path="src/user_tb.sv", line=4, reason="intentional")]
        """)
    check("waiver by path+line", files_with(found, "SV-COR-008") == ["vendor_vip.sv"], out + err)

    t, rc, found, out, err = case(binary, "Class-style waivers", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, Waiver
        class VendorNoise(Waiver):
            rules = ["SelfComparison", "SV-STY-016"]
            paths = ["lib/**"]
            reason = "vendor code"
            owner = "dv-infra"
        class Stale(Waiver):
            rules = ["SV-COR-001"]
            reason = "temporary"
            expires = "2001-01-01"
        class Project(ProjectConfig):
            waivers = [VendorNoise, Stale]
        """)
    check("class waiver by rule name and glob", files_with(found, "SV-COR-008") == ["user_tb.sv"] and
          files_with(found, "SV-STY-016") == ["user_tb.sv"], out + err)
    check("expired waiver no longer applies", files_with(found, "SV-COR-001") == ["user_tb.sv", "vendor_vip.sv"], out)
    check("expired waiver is reported", "expired on 2001-01-01 (owner" not in err and "Stale expired on 2001-01-01" in err, err)

    t, rc, found, out, err = case(binary, "Waiver without reason", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, Waiver
        class Project(ProjectConfig):
            waivers = [Waiver("SV-COR-008", reason="")]
        """)
    check("missing reason is an error (exit 2)", rc == 2 and "non-empty reason" in err, f"rc={rc}\n{err}")

    # 7. Library macros expanded in user code.
    macro = {"lib/lib_macros.svh": LIB_MACRO, "src/user_macro.sv": USER_WITH_MACRO}
    cfg = """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            library_paths = ["lib"]
            report_library_macro_expansions = {flag}
        """
    t, rc, found, out, err = case(binary, "Library macro expansion (default)", "uvmcheck_config.py",
                                  cfg.format(flag="False"), args=["src/user_macro.sv", "-I", "lib"], extra=macro)
    check("finding inside library macro suppressed", files_with(found, "SV-COR-008") == [], out + err)
    t, rc, found, out, err = case(binary, "Library macro expansion (reported)", "uvmcheck_config.py",
                                  cfg.format(flag="True"), args=["src/user_macro.sv", "-I", "lib"], extra=macro)
    check("report_library_macro_expansions=True reports it",
          files_with(found, "SV-COR-008") == ["user_macro.sv"], out + err)

    # 8. CLI flags and errors.
    t, rc, found, out, err = case(binary, "--library on the command line", None, "",
                                  args=["lib/vendor_vip.sv", "src/user_tb.sv", "--library", "lib/*.sv"])
    check("--library glob", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    # 9. Build settings: sources / filelists / defines from the config, paths
    #    relative to the project root for a config inside .uvmcheck/.
    t, rc, found, out, err = case(binary, "Config sources (no files on the command line)",
                                  ".uvmcheck/config.py", """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            sources = ["src/*.sv"]
            library_paths = ["lib"]
        """, args=[])
    check("config sources compiled when no files given", rc != 2 and files_with(found, "SV-COR-008") == ["user_tb.sv"],
          f"rc={rc}\n{out}{err}")

    t, rc, found, out, err = case(binary, "Config filelist + define", ".uvmcheck/config.json",
                                  '{"filelists": ["files.f"], "defines": ["WITH_EXTRA"]}', args=[],
                                  extra={"files.f": "src/user_tb.sv\nsrc/extra.sv\n",
                                         "src/extra.sv": "`ifdef WITH_EXTRA\nmodule extra; int x; initial x = x; endmodule\n`endif\n"})
    check("filelist read relative to the list", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)
    check("define applied (guarded module compiled)", "extra.sv" in {f for f, _, _ in found}, out + err)

    t, rc, found, out, err = case(binary, "Files on the command line win over config sources",
                                  ".uvmcheck/config.json", '{"sources": ["lib/*.sv"]}',
                                  args=["src/user_tb.sv"])
    check("config sources ignored when files are given", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    t, rc, found, out, err = case(binary, "Broken JSON", ".uvmcheck.json", '{\n  "library_paths": [lib]\n}')
    check("invalid JSON fails with line:col", rc == 2 and ":2:" in err, f"rc={rc}\n{err}")

    t, rc, found, out, err = case(binary, "Python exception", "uvmcheck_config.py", "raise RuntimeError('boom')\n")
    check("python traceback surfaced, exit 2", rc == 2 and "boom" in err, f"rc={rc}\n{err}")

    t, rc, found, out, err = case(binary, "Typo in rule key", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig, RuleConfig
        class Project(ProjectConfig):
            class MaxLineLenght(RuleConfig):
                enabled = False
        """)
    check("unknown rule warns", "'MaxLineLenght' does not match any rule" in err, err)

    t, rc, found, out, err = case(binary, "Two candidate configs", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig
        class A(ProjectConfig):
            library_paths = ["lib"]
        class B(ProjectConfig):
            pass
        """)
    check("ambiguous config is an error", rc == 2 and "set 'config = " in err, err)

    t, rc, found, out, err = case(binary, "Explicit config selection", "uvmcheck_config.py", """\
        from uvmcheck.config import ProjectConfig
        class A(ProjectConfig):
            library_paths = ["lib"]
        class B(ProjectConfig):
            pass
        config = A
        """)
    check("config = A picks A", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

    # 17. Directory-scoped .uvmcheck/config.py auto-discovery
    t, rc, found, out, err = case(binary, "Directory-scoped .uvmcheck/config.py auto-discovery", ".uvmcheck/config.py", """\
        from uvmcheck.config import ProjectConfig
        class Project(ProjectConfig):
            library_paths = ["lib"]
        """)
    check("user file reported under .uvmcheck/config.py", files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)
    check(".uvmcheck/config.py auto-discovered", ".uvmcheck/config.py" in out, out)

    # 18. Auto-discovery of .uvmcheck/waivers.json
    waiver_json = json.dumps({
        "version": "1.0",
        "waivers": [
            {"rule": "SV-COR-008", "path": "src/user_tb.sv", "reason": "waived"}
        ]
    })
    t, rc, found, out, err = case(binary, ".uvmcheck/waivers.json auto-discovery", None, "", extra={
        ".uvmcheck/waivers.json": waiver_json
    })
    check("violation waived by auto-discovered .uvmcheck/waivers.json", files_with(found, "SV-COR-008") == ["vendor_vip.sv"], out + err)
    check("waiver file auto-loaded banner present", "Loaded external waivers from .uvmcheck/waivers.json" in out, out)

    # 10. Discovery from the sources, and layered sub-tree configs.
    with tempfile.TemporaryDirectory() as d:
        root = Path(d) / "proj"
        root.mkdir()
        make_project(root)
        (root / ".uvmcheck").mkdir()
        (root / ".uvmcheck" / "config.py").write_text(textwrap.dedent("""\
            from uvmcheck.config import ProjectConfig
            class Project(ProjectConfig):
                library_paths = ["lib/**"]
            """))
        sim = Path(d) / "sim"   # outside the project tree
        sim.mkdir()
        rc, found, out, err = run(binary, sim, [str(root / "lib/vendor_vip.sv"), str(root / "src/user_tb.sv")])
        print("\nConfig found from the source files (run outside the tree)")
        check("config discovered from the first source", "Auto-discovered configuration" in out and
              files_with(found, "SV-COR-008") == ["user_tb.sv"], out + err)

        blk = root / "blk"
        (blk / ".uvmcheck").mkdir(parents=True)
        (blk / ".uvmcheck" / "config.py").write_text(textwrap.dedent("""\
            from uvmcheck.config import parent_project, RuleConfig
            class Block(parent_project()):
                class SelfComparison(RuleConfig):
                    enabled = False
            """))
        rc, found, out, err = run(binary, blk, ["../lib/vendor_vip.sv", "../src/user_tb.sv"])
        print("\nLayered config (block config extends the project config)")
        check("layers reported", "Configuration layers:" in out, out + err)
        check("parent's relative library path still points at the parent's lib/",
              "vendor_vip.sv" not in {f for f, _, _ in found}, out + err)
        check("block's own override applies", files_with(found, "SV-COR-008") == [], out + err)
        check("parent's rules still run", files_with(found, "SV-COR-001") == ["user_tb.sv"], out + err)

    print(f"\n{'FAILED' if failures else 'PASSED'}: {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
