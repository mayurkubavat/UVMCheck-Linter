#!/usr/bin/env python3
"""
Integration tests for the git-history rules: SV-EVO-001 (hotspots) and SV-EVO-002 (change
coupling). Each case builds a small git repository in a temp directory, commits to it, and
runs uvmcheck with --history. Without --history the rules must stay silent.
"""

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import textwrap
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
VIOLATION_RE = re.compile(r"^(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+): \[(?P<sev>[A-Z]+)\] (?P<rule>[A-Z0-9.\-_]+) ")

failures = []


def check(name, cond, detail=""):
    print(("  PASS  " if cond else "  FAIL  ") + name)
    if not cond:
        failures.append(name)
        if detail:
            print(textwrap.indent(detail.strip(), "        "))


def git(root, *args):
    env = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.com",
               GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.com")
    subprocess.run(["git", "-C", str(root)] + list(args), check=True, capture_output=True, env=env)


def commit(root, files, message):
    for name, text in files.items():
        (root / name).write_text(text)
    git(root, "add", "-A")
    git(root, "commit", "-q", "-m", message)


def run(binary, root, args):
    res = subprocess.run([binary] + args, cwd=root, capture_output=True, text=True)
    out = re.sub(r"\x1b\[[0-9;]*m", "", res.stdout)
    found = []
    for line in out.splitlines():
        m = VIOLATION_RE.match(line)
        if m:
            found.append((os.path.basename(m["file"]), m["rule"], line))
    return found, out + res.stderr


def branchy(name, n, tag):
    """A function with cyclomatic complexity n + 1; tag changes its text between commits."""
    arms = "\n".join(f"    if (x == {i}) r = r + {i * tag};" for i in range(n))
    return f"  function automatic int {name}(int x);\n    int r = 0;\n{arms}\n    return r;\n  endfunction\n"


def package(name, body):
    return f"package {name};\n{body}endpackage\n"


def klass(name, body=""):
    return f"  class {name};\n    int v;\n{body}  endclass\n"


def hotspot_case(binary, work):
    root = work / "hotspot"
    root.mkdir()
    git(root, "init", "-q")
    # busy.sv: complex and changed in every commit. calm.sv: as complex, committed once.
    # small.sv: changed every time but simple.
    for i in range(1, 5):
        files = {
            "busy.sv": package("busy_pkg", branchy("decode", 20, i)),
            "small.sv": package("small_pkg", f"  localparam int REV = {i};\n  function automatic int f(); return REV; endfunction\n"),
        }
        if i == 1:
            files["calm.sv"] = package("calm_pkg", branchy("route", 20, 1))
        commit(root, files, f"change {i}")
    srcs = ["busy.sv", "calm.sv", "small.sv"]

    found, out = run(binary, root, srcs + ["--rule", "SV-EVO-001"])
    check("hotspot: silent without --history", found == [], out)

    found, out = run(binary, root, srcs + ["--history", "--rule", "SV-EVO-001"])
    files = [f for f, r, _ in found if r == "SV-EVO-001"]
    check("hotspot: the complex, often-changed file is reported", files == ["busy.sv"], out)
    check("hotspot: message gives churn and complexity",
          any("changed in 4 of the last 4 commits" in l and "complexity 21" in l and "'decode'" in l
              for _, _, l in found), out)

    cfg = root / ".uvmcheck.json"
    cfg.write_text('{ "rules": { "SV-EVO-001": { "options": { "min_commits": 1 } } } }')
    found, out = run(binary, root, srcs + ["--history", "--rule", "SV-EVO-001"])
    files = [f for f, r, _ in found if r == "SV-EVO-001"]
    check("hotspot: min_commits option, ranked by score", files == ["busy.sv", "calm.sv"], out)
    cfg.write_text('{ "rules": { "SV-EVO-001": { "options": { "min_commits": 1, "top_files": 1 } } } }')
    found, out = run(binary, root, srcs + ["--history", "--rule", "SV-EVO-001"])
    check("hotspot: top_files option", [f for f, r, _ in found if r == "SV-EVO-001"] == ["busy.sv"], out)

    # Outside a repository there is no history and no finding.
    plain = work / "plain"
    plain.mkdir()
    shutil.copy(root / "busy.sv", plain / "busy.sv")
    found, out = run(binary, plain, ["busy.sv", "--history", "--rule", "SV-EVO-001"])
    check("hotspot: file outside a git repository is skipped", found == [], out)


def coupling_case(binary, work):
    root = work / "coupling"
    (root / "sub dir").mkdir(parents=True)
    git(root, "init", "-q")
    names = ["agent.sv", "env.sv", "sub dir/test.sv", "seq.sv", "other.sv"]
    commit(root, {
        "agent.sv": package("agent_pkg", klass("bus_driver")),
        "env.sv": package("env_pkg", klass("bus_env")),
        "sub dir/test.sv": package("test_pkg", klass("bus_test")),
        "seq.sv": package("agent_pkg_seq", klass("bus_seq")),
        "other.sv": package("other_pkg", klass("other_comp")),
    }, "initial")
    # agent and env change together four times; test changes with agent once; seq changes alone.
    for i in range(4):
        commit(root, {
            "agent.sv": package("agent_pkg", klass("bus_driver", f"    int r{i};\n")),
            "env.sv": package("env_pkg", klass("bus_env", f"    int e{i};\n")),
        }, f"together {i}")
        commit(root, {"seq.sv": package("agent_pkg_seq", klass("bus_seq", f"    int s{i};\n"))}, f"alone {i}")
    commit(root, {
        "agent.sv": package("agent_pkg", klass("bus_driver", "    int z;\n")),
        "sub dir/test.sv": package("test_pkg", klass("bus_test", "    int z;\n")),
    }, "once")
    # A sweeping commit that touches every file does not count.
    commit(root, {n: (root / n).read_text() + "// header\n" for n in names}, "reformat")

    found, out = run(binary, root, names + ["--rule", "SV-EVO-002"])
    check("coupling: silent without --history", found == [], out)

    cfg = root / ".uvmcheck.json"
    cfg.write_text('{ "rules": { "SV-EVO-002": { "options": { "max_commit_files": 4 } } } }')
    found, out = run(binary, root, names + ["--history", "--rule", "SV-EVO-002"])
    pairs = [l for f, r, l in found if r == "SV-EVO-002"]
    check("coupling: one finding, for agent and env", len(pairs) == 1 and "'agent.sv'" in pairs[0]
          and "'env.sv'" in pairs[0], out)
    check("coupling: shared count and degree, sweeping commit left out",
          bool(pairs) and "in 4 commits, 88%" in pairs[0], out)

    cfg.write_text('{ "rules": { "SV-EVO-002": { "options": { "max_commit_files": 10 } } } }')
    found, out = run(binary, root, names + ["--history", "--rule", "SV-EVO-002"])
    pairs = [l for f, r, l in found if r == "SV-EVO-002"]
    check("coupling: max_commit_files lets the sweeping commits count",
          len(pairs) == 2 and "in 6 commits, 92%" in pairs[0] and "'test.sv'" in pairs[1], out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    args = ap.parse_args()
    binary = str(Path(args.binary).resolve())
    if shutil.which("git") is None:
        print("git not found; skipping history tests")
        return 0
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        print("SV-EVO-001 hotspots")
        hotspot_case(binary, work)
        print("SV-EVO-002 change coupling")
        coupling_case(binary, work)
    if failures:
        print(f"\n{len(failures)} history test(s) failed")
        return 1
    print("\nAll history tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
