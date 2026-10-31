#!/usr/bin/env python3
"""
Baseline and inline-waiver behaviour:

- a baseline keeps suppressing a finding after lines are inserted above it, and after the
  project folder moves;
- files with the same name in different folders get different baseline entries;
- changing the flagged line itself makes the finding reappear, and the old entry is
  reported as no longer matching;
- version 1.0 baselines (rule@basename:line) are still honoured;
- waiver files are parsed as JSON (a list, or {"waivers": [...]});
- `// uvmcheck-disable-next-line RULE` on the line above waives a finding.
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RULE = "SV-COR-008"  # SelfComparison: if (a == a)

MODULE = """module {name};
  int a;
  initial begin
{pad}    if (a == a) $display("same");
  end
endmodule : {name}
"""

failures = []


def check(cond, msg):
    if not cond:
        failures.append(msg)
        print(f"  FAIL: {msg}")
    else:
        print(f"  ok: {msg}")


def write(path: Path, name: str, pad_lines: int = 0):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(MODULE.format(name=name, pad="    // filler\n" * pad_lines))


def lint(binary, cwd, files, *extra):
    with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tmp:
        out = tmp.name
    res = subprocess.run([binary, "--rule", RULE, "--json", out, *extra, *files],
                         cwd=cwd, capture_output=True, text=True)
    report = json.loads(Path(out).read_text())
    Path(out).unlink()
    return [v for v in report["violations"] if v["ruleId"] == RULE], res.stdout + res.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", default=str(REPO / "build" / "uvmcheck"))
    binary = str(Path(ap.parse_args().binary).resolve())

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "proj"
        (root / ".uvmcheck").mkdir(parents=True)
        (root / ".uvmcheck" / "config.py").write_text(
            "from uvmcheck.config import ProjectConfig\nclass Project(ProjectConfig):\n    pass\n")
        write(root / "agent_a" / "driver.sv", "drv_a")
        write(root / "agent_b" / "driver.sv", "drv_b")
        files = ["agent_a/driver.sv", "agent_b/driver.sv"]

        found, _ = lint(binary, root, files)
        check(len(found) == 2, "two findings before the baseline")

        lint(binary, root, files, "--update-baseline", ".uvmcheck/baseline.json")
        data = json.loads((root / ".uvmcheck" / "baseline.json").read_text())
        fps = {e["fingerprint"] for e in data["baseline"]}
        check(data["version"] == "2.0", "baseline is version 2.0")
        check(len(fps) == 2, "same-named files in different folders get distinct fingerprints")
        check({e["file"] for e in data["baseline"]} == set(files), "baseline paths are relative to the project root")

        found, _ = lint(binary, root, files)
        check(found == [], "baseline suppresses both findings")

        write(root / "agent_a" / "driver.sv", "drv_a", pad_lines=5)
        found, _ = lint(binary, root, files)
        check(found == [], "finding stays suppressed after lines are inserted above it")

        moved = Path(tmp) / "moved"
        shutil.copytree(root, moved)
        found, _ = lint(binary, moved, files)
        check(found == [], "baseline still matches after the project folder moves")

        text = (root / "agent_b" / "driver.sv").read_text().replace("if (a == a)", "if (a == a && 1)")
        (root / "agent_b" / "driver.sv").write_text(text)
        found, log = lint(binary, root, files)
        check(len(found) == 1 and found[0]["file"].endswith("agent_b/driver.sv"),
              "editing the flagged line brings the finding back")
        check("1 baseline entry matched no finding" in log, "the stale entry is reported")

        # Version 1.0 baselines keep working.
        legacy = {"version": "1.0", "baseline": [
            {"rule": RULE, "file": "agent_b/driver.sv", "line": 4, "fingerprint": f"{RULE}@driver.sv:4"}]}
        (root / "legacy.json").write_text(json.dumps(legacy))
        found, _ = lint(binary, root, ["agent_b/driver.sv"], "--baseline", "legacy.json")
        check(found == [], "version 1.0 fingerprints are still honoured")

        # Waiver files are real JSON, in either shape.
        for i, doc in enumerate(([{"rule": RULE, "path": "agent_b/*", "reason": "known"}],
                                 {"waivers": [{"rule": RULE, "file": "agent_b/driver.sv"}]})):
            (root / f"w{i}.json").write_text(json.dumps(doc))
            found, _ = lint(binary, root, ["agent_b/driver.sv"], "--baseline", "legacy-none.json",
                            "--waivers", f"w{i}.json")
            check(found == [], f"waiver file shape {i + 1} is applied")

        # Inline waiver on the line above.
        nl = Path(tmp) / "nl"
        nl.mkdir()
        (nl / "t.sv").write_text(MODULE.format(name="t", pad="").replace(
            "    if (a == a)", f"    // uvmcheck-disable-next-line {RULE}\n    if (a == a)"))
        found, _ = lint(binary, nl, ["t.sv"])
        check(found == [], "uvmcheck-disable-next-line waives the next line")

    print(f"baseline tests: {len(failures)} failure(s)")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
