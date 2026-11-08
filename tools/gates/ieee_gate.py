#!/usr/bin/env python3
"""IEEE 1800 conformance gate for the native builder (dv-build) against Slang.

Runs every sv-tests file through dv-build and through uvmcheck (Slang) and
reports, per LRM clause (the sv-tests chapter):
  legal        tests that must compile: how many each front end accepts;
  should_fail  tests that must be rejected: how many each front end rejects.
A legal test that dv-build rejects is a native false error and fails the
gate; a should_fail test only Slang rejects is a known gap, listed with
--gaps. Runtime-only should_fail tests (`:type: simulation` alone) are
counted apart: no front end can reject them.

    cmake --build build && cargo build --release -p dv-build
    UVM_SRC=uvm-core/src tools/gates/ieee_gate.py ~/sv-tests [--gaps] [chapter-8]
"""
import collections, concurrent.futures, json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
UVMCHECK = os.environ.get('UVMCHECK', os.path.join(REPO, 'build', 'uvmcheck'))
DVBUILD = os.environ.get('DV_BUILD', os.path.join(REPO, 'target', 'release', 'dv-build'))

CLAUSES = {
    '5': 'Lexical conventions', '6': 'Data types', '7': 'Aggregate data types', '8': 'Classes',
    '9': 'Processes', '10': 'Assignment statements', '11': 'Operators and expressions',
    '12': 'Procedural programming statements', '13': 'Tasks and functions', '14': 'Clocking blocks',
    '15': 'Interprocess synchronization', '16': 'Assertions', '18': 'Constrained random value generation',
    '20': 'Utility system tasks and functions', '21': 'Input/output system tasks', '22': 'Compiler directives',
    '23': 'Modules and hierarchy', '24': 'Programs', '25': 'Interfaces', '26': 'Packages',
}


def header(f):
    h = open(f, errors='replace').read()
    tag = lambda k: (re.search(rf'^:{k}:(.*)$', h, re.M) or [None, ''])[1].strip()
    return h, tag


def args_for(root, f, h):
    a = []
    m = re.search(r'^:defines:(.*)$', h, re.M)
    if m:
        a += ['+define+' + d for d in m.group(1).split()]
    m = re.search(r'^:incdirs:(.*)$', h, re.M)
    if m:
        a += ['+incdir+' + os.path.join(root, d) for d in m.group(1).split()]
    a += ['+incdir+' + os.path.dirname(f)]
    if os.environ.get('UVM_SRC'):
        a += ['+incdir+' + os.environ['UVM_SRC']]
        # As the sv-tests runner does: UVM tests compile uvm_pkg first.
        if re.search(r'^:tags:.*\buvm\b', h, re.M) or 'uvm_pkg' in h:
            a += [os.path.join(os.environ['UVM_SRC'], 'uvm_pkg.sv')]
    return a + [f]


def slang_rejects(args):
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, 'r.json')
        subprocess.run([UVMCHECK, '--no-config', '--no-cache', '--json', out, *args], capture_output=True,
                       text=True, errors='replace', timeout=120)
        try:
            return json.load(open(out)).get('compileErrors', 0) > 0
        except (OSError, ValueError):
            return True


def native_rejects(args):
    r = subprocess.run([DVBUILD, '--quiet', *args], capture_output=True, text=True, errors='replace', timeout=120)
    return r.returncode != 0


def run(root, f):
    h, tag = header(f)
    rel = os.path.relpath(f, os.path.join(root, 'tests'))
    top = rel.split('/')[0]
    clause = top.split('-')[1] if top.startswith('chapter-') else top
    bad = ':should_fail' in h
    kind = 'should_fail' if bad else 'legal'
    if bad and tag('type') == 'simulation':
        kind = 'runtime'
    a = args_for(root, f, h)
    return rel, clause, kind, native_rejects(a), slang_rejects(a), tag('should_fail_because')


def main():
    argv = [x for x in sys.argv[1:] if not x.startswith('--')]
    gaps = '--gaps' in sys.argv
    root, sel = argv[0], (argv[1] if len(argv) > 1 else '')
    import glob
    files = [f for f in sorted(glob.glob(os.path.join(root, 'tests', '**', '*.sv'), recursive=True)) if sel in f]
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as pool:
        rows = list(pool.map(lambda f: run(root, f), files))
    t = collections.defaultdict(collections.Counter)
    for rel, clause, kind, nat, sl, why in rows:
        c = t[clause]
        c[kind] += 1
        c[kind + '_native'] += (not nat) if kind == 'legal' else nat
        c[kind + '_slang'] += (not sl) if kind == 'legal' else sl
    key = lambda c: (0, int(c)) if c.isdigit() else (1, c)
    print(f'{"clause":<8} {"title":<36} {"legal ok (native/slang/all)":>28} {"rejected (native/slang/all)":>28}')
    tot = collections.Counter()
    for clause in sorted(t, key=key):
        c = t[clause]
        tot.update(c)
        print(f'{clause:<8} {CLAUSES.get(clause, ""):<36} '
              f'{c["legal_native"]:>9}/{c["legal_slang"]}/{c["legal"]:<12} '
              f'{c["should_fail_native"]:>9}/{c["should_fail_slang"]}/{c["should_fail"]:<12}')
    print(f'{"total":<45} {tot["legal_native"]:>9}/{tot["legal_slang"]}/{tot["legal"]:<12} '
          f'{tot["should_fail_native"]:>9}/{tot["should_fail_slang"]}/{tot["should_fail"]:<12}')
    print(f'runtime-only should_fail tests (no front end can reject): {tot["runtime"]}')
    false_errors = [r for r in rows if r[2] == 'legal' and r[3]]
    for r in false_errors:
        print('NATIVE FALSE ERROR', r[0])
    if gaps:
        for r in rows:
            if r[2] == 'should_fail' and r[4] and not r[3]:
                print('GAP', r[0], '|', r[5][:100])
    return 1 if false_errors else 0


if __name__ == '__main__':
    sys.exit(main())
