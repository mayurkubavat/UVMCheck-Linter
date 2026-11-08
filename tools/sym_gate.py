#!/usr/bin/env python3
"""Symbol gate for builder phase N1: the scopes dv-build builds against Slang's.

For every legal sv-tests file (or each file given with --files), runs
`dv-build --symbols` and build/uvmcheck_sym_dump on the same options and
compares the sorted `<scope path> <kind> <name>` lines. Prints per clause
how many files match, then the most common differing lines (--show=N
prints the first N differing files in full). --types also compares each
value's and typedef's type (builder phase N2); --values each parameter's,
specparam's and enum value's value (phase N3). --hier compares the
elaborated design instead (`dv-build --hier`, phase N4).

    cmake --build build --target uvmcheck_sym_dump && cargo build --release -p dv-build   # in rust/
    UVM_SRC=uvm-core/src tools/sym_gate.py ~/sv-tests [--types] [--values] [--hier] [--show=N] [chapter-8]
    tools/sym_gate.py --repo UVM_SRC [--show=N]      # each repo test and example alone
    tools/sym_gate.py --files [options] FILE...      # one design
"""
import collections, concurrent.futures, os, re, subprocess, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ieee_gate import DVBUILD, REPO, args_for, header  # noqa: E402

SYMDUMP = os.environ.get('SYM_DUMP', os.path.join(REPO, 'build', 'uvmcheck_sym_dump'))


# Slang numbers anonymous enums, structs and unions (`e$3`, `s$7`) with
# counters that span the whole compilation; only the kind is compared.
ANON = re.compile(r'([eus])\$\d+')


# Slang reuses the first specialization made with the same parameter values
# and prints that one's arguments (`uvm_driver#(uvm_sequence_item,
# uvm_pkg::uvm_driver::REQ)` for one written with two explicit arguments).
# These typedefs' types are not compared.
CACHED_SPECIALIZATIONS = {
    'uvm_pkg typedef ' + n for n in (
        'uvm_default_driver_type', 'uvm_default_sequence_type', 'uvm_default_sequencer_param_type',
        'uvm_default_sequencer_type', 'uvm_id_file_array', 'uvm_id_verbosities_array')
}


def normalize(l):
    l = ANON.sub(r'\1$', l)
    head = l.split(' : ', 1)[0]
    return head if head in CACHED_SPECIALIZATIONS else l


def dump(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace', timeout=300)
    return sorted(normalize(l) for l in r.stdout.splitlines() if l and not l.startswith('errors '))


TYPES = '--types' in sys.argv
VALUES = '--values' in sys.argv
HIER = '--hier' in sys.argv


def compare(args):
    flag = (['--types'] if TYPES else []) + (['--values'] if VALUES else []) + (['--hier'] if HIER else [])
    native = dump([DVBUILD, '--quiet', '--symbols', *flag, *args])
    slang = dump([SYMDUMP, *flag, *args])
    n, s = collections.Counter(native), collections.Counter(slang)
    return sorted((n - s).elements()), sorted((s - n).elements())


def main():
    show = next((int(a.split('=')[1]) for a in sys.argv if a.startswith('--show=')), 0)
    if '--files' in sys.argv:
        extra, missing = compare([a for a in sys.argv[1:] if not a.startswith('--')])
        for l in extra:
            print('+ native', l)
        for l in missing:
            print('- native', l)
        print(f'extra {len(extra)} missing {len(missing)}')
        return 1 if extra or missing else 0
    argv = [x for x in sys.argv[1:] if not x.startswith('--')]
    import glob
    cases = []  # (label, group, args)
    if '--repo' in sys.argv:
        uvm = argv[0]
        for f in sorted(glob.glob(os.path.join(REPO, 'tests/**/*.sv*'), recursive=True)
                        + glob.glob(os.path.join(REPO, 'examples/**/*.sv*'), recursive=True)):
            rel = os.path.relpath(f, REPO)
            cases.append((rel, rel.split('/')[0], ['+incdir+' + uvm, '+incdir+' + os.path.join(REPO, 'testdata/common'),
                                                   '+incdir+' + os.path.dirname(f), f]))
    else:
        root, sel = argv[0], (argv[1] if len(argv) > 1 else '')
        for f in sorted(glob.glob(os.path.join(root, 'tests', '**', '*.sv'), recursive=True)):
            h, _ = header(f)
            if sel in f and ':should_fail' not in h:
                rel = os.path.relpath(f, os.path.join(root, 'tests'))
                top = rel.split('/')[0]
                cases.append((rel, top.split('-')[1] if top.startswith('chapter-') else top, args_for(root, f, h)))

    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as pool:
        rows = list(pool.map(lambda c: (c[0], c[1], compare(c[2])), cases))
    per = collections.defaultdict(collections.Counter)
    lines = collections.Counter()
    bad = []
    for rel, clause, (extra, missing) in rows:
        ok = not extra and not missing
        per[clause]['files'] += 1
        per[clause]['ok'] += ok
        if not ok:
            bad.append((rel, extra, missing))
            for l in extra:
                lines['+ ' + l.split(' ', 1)[1]] += 1
            for l in missing:
                lines['- ' + l.split(' ', 1)[1]] += 1
    key = lambda c: (0, int(c)) if c.isdigit() else (1, c)
    tot = collections.Counter()
    for c in sorted(per, key=key):
        tot.update(per[c])
        print(f'{c:<12} {per[c]["ok"]:>5}/{per[c]["files"]}')
    print(f'{"total":<12} {tot["ok"]:>5}/{tot["files"]}')
    for l, n in lines.most_common(25):
        print(f'{n:>5} {l}')
    for rel, extra, missing in bad[:show]:
        print('FILE', rel)
        for l in extra:
            print('  + native', l)
        for l in missing:
            print('  - native', l)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
