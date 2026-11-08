#!/usr/bin/env python3
"""Build uvmcheck against slang master and report what changed.

Run weekly (or before bumping the pin in third_party/slang/slang.cmake). It
  1. clones or updates slang at --ref (default: master) under --work, and
     applies our patches (third_party/slang/patches/*.patch), reporting each
     as applying, already upstream (delete it) or not applying (rebase it),
  2. configures a separate build with FETCHCONTENT_SOURCE_DIR_SLANG pointing at
     that checkout, so the pinned build is untouched,
  3. runs the suites that measure slang itself: the LRM conformance probes, the
     overlay test (which fails when an override has outlived its slang bug, or a
     known crash no longer crashes), the rule suites and, with --uvm-home, the
     real-UVM suite,
  4. writes a Markdown report: the slang commit, each patch and suite result,
     and the failing lines, ready to paste into the pin-bump commit or an issue.

  tools/quality/slang_canary.py --work ../canary [--ref master] [--uvm-home ../uvm-core] [--report canary.md]

Exit status: 0 when every suite passes on the new slang (the pin can move),
1 when one fails, 2 when slang or uvmcheck does not build.
"""

import argparse
import datetime
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SLANG_URL = "https://github.com/MikePopoloski/slang.git"
SUITES = ["lrm_conformance_test", "slang_overlay_test", "rule_regression_suite", "all_rules_stub_suite",
          "engine_equivalence", "ast_coverage_floor_test"]


def sh(cmd, cwd=None, log=None):
    res = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    if log is not None:
        log.write(f"$ {' '.join(map(str, cmd))}\n{res.stdout}{res.stderr}\n")
    return res


def pinned_commit():
    for line in (ROOT / "third_party" / "slang" / "slang.cmake").read_text().splitlines():
        if line.startswith("set(UVMCHECK_SLANG_TAG"):
            return line.split()[1].rstrip(")")
    return "?"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--work", required=True, help="folder for the slang checkout and the canary build")
    ap.add_argument("--ref", default="master", help="slang branch, tag or commit (default: master)")
    ap.add_argument("--uvm-home", help="uvm-core checkout, to add the real-UVM suite")
    ap.add_argument("--report", default="slang_canary.md", help="Markdown report path")
    ap.add_argument("-j", "--jobs", default="8")
    opts = ap.parse_args()

    work = Path(opts.work).resolve()
    work.mkdir(parents=True, exist_ok=True)
    slang, build = work / "slang", work / "build"
    logf = open(work / "canary.log", "w")

    if not (slang / ".git").exists():
        res = sh(["git", "clone", "--quiet", SLANG_URL, str(slang)], log=logf)
        if res.returncode:
            print("could not clone slang; see canary.log", file=sys.stderr)
            return 2
    sh(["git", "fetch", "--quiet", "origin"], cwd=slang, log=logf)
    sh(["git", "reset", "--quiet", "--hard"], cwd=slang, log=logf)  # drop patches from the last run
    sh(["git", "checkout", "--quiet", "--detach", f"origin/{opts.ref}"], cwd=slang, log=logf).returncode and \
        sh(["git", "checkout", "--quiet", "--detach", opts.ref], cwd=slang, log=logf)
    head = sh(["git", "log", "-1", "--format=%H %cs %s"], cwd=slang).stdout.strip()

    # Our local patches (third_party/slang/patches), as the pinned build applies
    # them: FETCHCONTENT_SOURCE_DIR_SLANG skips CMake's own patch step.
    patches = []
    for p in sorted((ROOT / "third_party" / "slang" / "patches").glob("*.patch")):
        if sh(["git", "apply", "--reverse", "--check", str(p)], cwd=slang).returncode == 0:
            patches.append((p.name, "already upstream: delete the patch"))
        elif sh(["git", "apply", "--whitespace=nowarn", str(p)], cwd=slang, log=logf).returncode == 0:
            patches.append((p.name, "applies"))
        else:
            patches.append((p.name, "DOES NOT APPLY: rebase it on this commit"))

    cfg = ["cmake", "-S", str(ROOT), "-B", str(build), "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release",
           f"-DFETCHCONTENT_SOURCE_DIR_SLANG={slang}"]
    if opts.uvm_home:
        cfg.append(f"-DUVMCHECK_UVM_HOME={Path(opts.uvm_home).resolve()}")
    suites = SUITES + (["all_rules_uvm_core_suite"] if opts.uvm_home else [])
    results, built = {}, False
    if sh(cfg, log=logf).returncode == 0 and sh(["cmake", "--build", str(build), "-j", opts.jobs],
                                                 log=logf).returncode == 0:
        built = True
        for suite in suites:
            res = sh(["ctest", "--test-dir", str(build), "-R", f"^{suite}$", "--output-on-failure"], log=logf)
            fails = [ln.strip() for ln in (res.stdout + res.stderr).splitlines()
                     if "FAIL" in ln and "Failed  " not in ln][:15]
            results[suite] = (res.returncode == 0, fails)

    lines = [f"# Slang canary, {datetime.date.today().isoformat()}", "",
             f"- pinned: `{pinned_commit()}`", f"- tested: `{head}` ({opts.ref})", ""]
    if patches:
        lines += ["| Local patch | On this commit |", "| --- | --- |"]
        lines += [f"| {n} | {st} |" for n, st in patches] + [""]
    if not built:
        lines += ["**Build failed.** slang's API changed in a way uvmcheck does not compile against; "
                  f"see `{work / 'canary.log'}`."]
    else:
        lines += ["| Suite | Result |", "| --- | --- |"]
        lines += [f"| {s} | {'pass' if ok else 'FAIL'} |" for s, (ok, _) in results.items()]
        for s, (ok, fails) in results.items():
            if not ok:
                lines += ["", f"## {s}", "", "```", *fails, "```"]
        if all(ok for ok, _ in results.values()):
            lines += ["", f"Every suite passes: the pin in third_party/slang/slang.cmake can move to `{head.split()[0]}`."]
        else:
            lines += ["", "A failing overlay check that names an override or a slang issue usually means slang "
                      "fixed the bug: delete the override or retire the crash pattern, then move the pin."]
    Path(opts.report).write_text("\n".join(lines) + "\n")
    print("\n".join(lines))
    if not built:
        return 2
    return 0 if all(ok for ok, _ in results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
