#!/usr/bin/env python3
"""Parity of the native class model (sv-sema) with the Slang-backed one.

Both sides print dv_model::Design::dump through the model_dump example;
paths are cut to the file name since Slang reports them relative.

    cmake --preset model && cmake --build build-model
    cargo build --release -p sv-sema --example model_dump     # in rust/
    tools/model_parity.py --uvm UVM_SRC                         # uvm_pkg.sv
    tools/model_parity.py --repo UVM_SRC                        # tests/ and examples/
    tools/model_parity.py -- +incdir+DIR FILE...                # any compile

--bodies also compares method bodies (calls, assignments, timing, super.new),
--types the type strings. --show=N prints the first N lines of each diff. Lines are compared as
multisets, so the count is how many member lines one side lacks.
"""
import collections, difflib, glob, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
DUMP = os.environ.get('MODEL_DUMP', os.path.join(REPO, 'rust', 'target', 'release', 'examples', 'model_dump'))


def norm(text):
    return re.sub(r'@[^@\s]*/([^/\s:]+):', r'@\1:', text).splitlines()


FLAGS = []


def both(args):
    run = lambda extra: norm(subprocess.run([DUMP] + FLAGS + extra + args, capture_output=True,
                                            text=True, errors='replace').stdout)
    return run(['--slang']), run([])


def main():
    argv = sys.argv[1:]
    while argv and argv[0] in ('--bodies', '--types'):
        FLAGS.append(argv.pop(0))
    show = 0
    if argv and argv[0].startswith('--show='):
        show = int(argv[0][7:])
        argv = argv[1:]
    if argv[:1] == ['--repo']:
        uvm = argv[1]
        files = sorted(glob.glob(os.path.join(REPO, 'tests/**/*.sv*'), recursive=True)
                       + glob.glob(os.path.join(REPO, 'examples/**/*.sv*'), recursive=True))
        bad = 0
        for f in files:
            a, b = both(['+incdir+' + uvm, '+incdir+' + os.path.join(REPO, 'testdata/common'),
                         '+incdir+' + os.path.dirname(f), f])
            if a != b:
                bad += 1
                print('DIFF', os.path.relpath(f, REPO))
                for line in list(difflib.unified_diff(a, b, lineterm='', n=0))[:show]:
                    print('  ' + line)
        print(f'files={len(files)} identical={len(files) - bad} differ={bad}')
        return 1 if bad else 0
    if argv[:1] == ['--uvm']:
        args = ['+incdir+' + argv[1], os.path.join(argv[1], 'uvm_pkg.sv')]
    else:
        args = argv[1:] if argv[:1] == ['--'] else argv
    a, b = both(args)
    classes = lambda x: [l for l in x if l.startswith('class ')]
    ca, cb = collections.Counter(a), collections.Counter(b)
    only_slang, only_native = list((ca - cb).elements()), list((cb - ca).elements())
    diff = ['-' + l for l in only_slang] + ['+' + l for l in only_native]
    kinds = collections.Counter(l.split()[0] for l in only_slang)
    print(f'lines slang={len(a)} native={len(b)} classes slang={len(classes(a))} '
          f'native={len(classes(b))} class lines differ={classes(a) != classes(b)} '
          f'lines only in slang={len(only_slang)} {dict(kinds)}')
    for line in diff[:show]:
        print('  ' + line)
    return 1 if diff else 0


if __name__ == '__main__':
    sys.exit(main())
