#!/usr/bin/env python3
"""Specialization counts: dv-lint's syntax engine (dv-lint --specs first column, which also
folds in body-level heuristics) or, with --model, sv-sema's (third column,
sv_sema::spec_counts: exact per class) against
slang's, as uvmcheck's AST walk sees them (build/specdump, see
tools/specdump.cpp). A generic class's body is visited once per count, so
every rule that walks bodies depends on these numbers.

    tools/spec_gate.py [--model] [--show=N] --repo UVM_SRC | --uvm UVM_SRC | --dir DIR
"""
import concurrent.futures, glob, os, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
SPECDUMP = os.environ.get('SPECDUMP', os.path.join(REPO, 'build', 'specdump'))
DVLINT = os.environ.get('DV_LINT', os.path.join(REPO, 'rust', 'target', 'release', 'dv-lint'))


COL = 'syntax'


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, errors='replace').stdout.splitlines()


def counts(args):
    ref = {}
    for l in run([SPECDUMP, *args]):
        c, k = l.split(maxsplit=1)
        ref[k] = int(c)
    ours = {}
    for l in run([DVLINT, '--specs', *args]):
        c, k, m = l.split()
        v = m if COL == 'model' else c
        ours[k] = int(v) if v.isdigit() else -1
    return ref, ours


def main():
    argv = sys.argv[1:]
    show = 0
    global COL
    if argv and argv[0] == '--model':
        COL = 'model'
        argv.pop(0)
    if argv and argv[0].startswith('--show='):
        show = int(argv.pop(0)[7:])
    if argv[:1] == ['--repo']:
        files = sorted(glob.glob(os.path.join(REPO, 'tests/**/*.sv*'), recursive=True)
                       + glob.glob(os.path.join(REPO, 'examples/**/*.sv*'), recursive=True))
        cases = [(os.path.relpath(f, REPO), ['+incdir+' + argv[1], '+incdir+' + os.path.join(REPO, 'tests/common'),
                                            '+incdir+' + os.path.dirname(f), f]) for f in files]
    elif argv[:1] == ['--dir']:
        files = sorted(glob.glob(os.path.join(argv[1], '**/*.sv'), recursive=True))
        cases = [(os.path.relpath(f, argv[1]), ['+incdir+' + os.path.dirname(f), f]) for f in files]
    elif argv[:1] == ['--uvm']:
        cases = [('uvm_pkg', ['+incdir+' + argv[1], os.path.join(argv[1], 'uvm_pkg.sv')])]
    else:
        sys.exit(__doc__)
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as pool:
        results = list(pool.map(lambda c: (c[0], counts(c[1])), cases))
    classes = good = 0
    bad_cases = 0
    for label, (ref, ours) in results:
        diffs = [(k, ref.get(k, 0), ours.get(k, 0)) for k in sorted(set(ref) | set(ours)) if ref.get(k, 0) != ours.get(k, 0)]
        classes += len(set(ref) | set(ours))
        good += len(set(ref) | set(ours)) - len(diffs)
        if diffs:
            bad_cases += 1
            if show:
                print(f'DIFF {label}')
                for k, r, o in diffs[:show]:
                    print(f'  {k}: slang {r}, ours {o}')
    print(f'compiles={len(cases)} differ={bad_cases} classes={classes} identical={good}')


if __name__ == '__main__':
    main()
