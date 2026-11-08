#!/usr/bin/env python3
"""Parity of dv-lint (the ported rules in Rust) with uvmcheck's C++ rules.

For each compile, runs `uvmcheck --preset all --experimental --json` and
keeps the findings of the ported rules, then runs dv-lint on the native model
(default) and, with --slang, on the slang-backed one (model rules only; syntax
rules need the native trees). Findings are compared as multisets of (file
name, line, column, rule, message).

    cmake --build build && cmake --build build-model
    cargo build --release -p dv-lint                  # in rust/
    tools/lint_parity.py --repo UVM_SRC [--slang] [--show=N]
    tools/lint_parity.py --uvm UVM_SRC [--slang] [--show=N]
    tools/lint_parity.py --dir DIR [--show=N]     # each .sv under DIR alone (sv-tests)
    tools/lint_parity.py -- +incdir+DIR FILE...
"""
import collections, glob, json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
UVMCHECK = os.environ.get('UVMCHECK', os.path.join(REPO, 'build', 'uvmcheck'))
DVLINT = os.environ.get('DV_LINT', os.path.join(REPO, 'rust', 'target', 'release', 'dv-lint'))
LINE = re.compile(r'^(.*?):(\d+):(\d+): \[(\w+)\] (\S+) \((\w+)\): (.*)$')

KINDS = dict((l.split()[0], l.split()[2]) for l in
             subprocess.run([DVLINT, '--list'], capture_output=True, text=True).stdout.splitlines())
RULES = set(KINDS)
if '--slang' in sys.argv:
    RULES = {r for r, k in KINDS.items() if k == 'model'}


def reference(args):
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, 'r.json')
        subprocess.run([UVMCHECK, '--no-config', '--no-cache', '--preset', 'all', '--experimental',
                        '--json', out, *args], capture_output=True, text=True, errors='replace')
        try:
            data = json.load(open(out))
        except (OSError, ValueError):
            return None
    return files_with_errors(collections.Counter(
        (os.path.basename(v['file']), v['line'], v['column'], v['ruleId'], v['message'].split('\n')[0])
        for v in data['violations'] if v['ruleId'] in RULES))


def files_with_errors(found):
    # Compiler errors carry each front end's own wording and positions:
    # compare which files have any.
    out = collections.Counter()
    for k, n in found.items():
        if k[3] == 'SV-CMP-001':
            out[(k[0], 0, 0, k[3], '')] = 1
        else:
            out[k] += n
    return out


def ported(args, slang):
    res = subprocess.run([DVLINT, *(['--slang'] if slang else []), *args], capture_output=True, text=True,
                         errors='replace')
    got = collections.Counter()
    for line in res.stdout.splitlines():
        m = LINE.match(line)
        if m:
            got[(os.path.basename(m[1]), int(m[2]), int(m[3]), m[5], m[7])] += 1
    return files_with_errors(got)


def compare(args, slang, show, label):
    ref = reference(args)
    if ref is None:
        print('NOREF', label)
        return None
    got = ported(args, slang)
    missing, extra = ref - got, got - ref
    if missing or extra:
        print(f'DIFF {label} missing={sum(missing.values())} extra={sum(extra.values())}')
        rows = [('-', k) for k in missing.elements()] + [('+', k) for k in extra.elements()]
        for sign, (f, l, c, rule, msg) in rows[:show]:
            print(f'  {sign} {f}:{l}:{c} {rule} {msg[:110]}')
    return ref, missing, extra


def main():
    argv = sys.argv[1:]
    slang, show = False, 0
    while argv and argv[0].startswith('--') and argv[0] not in ('--repo', '--uvm', '--dir', '--'):
        if argv[0] == '--slang':
            slang = True
        elif argv[0].startswith('--show='):
            show = int(argv[0][7:])
        argv.pop(0)
    if argv[:1] == ['--repo']:
        uvm = argv[1]
        files = sorted(glob.glob(os.path.join(REPO, 'tests/**/*.sv*'), recursive=True)
                       + glob.glob(os.path.join(REPO, 'examples/**/*.sv*'), recursive=True))
        cases = [(os.path.relpath(f, REPO), ['+incdir+' + uvm, '+incdir+' + os.path.join(REPO, 'testdata/common'),
                                            '+incdir+' + os.path.dirname(f), f]) for f in files]
    elif argv[:1] == ['--dir']:
        files = sorted(glob.glob(os.path.join(argv[1], '**/*.sv'), recursive=True))
        cases = [(os.path.relpath(f, argv[1]), ['+incdir+' + os.path.dirname(f), f]) for f in files]
    elif argv[:1] == ['--uvm']:
        cases = [('uvm_pkg', ['+incdir+' + argv[1], os.path.join(argv[1], 'uvm_pkg.sv')])]
    else:
        cases = [('args', argv[1:] if argv[:1] == ['--'] else argv)]
    total = bad = noref = 0
    findings = missing = extra = 0
    by_rule = collections.Counter()
    fired = collections.Counter()
    import concurrent.futures
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as pool:
        results = list(pool.map(lambda c: (c[0], compare(c[1], slang, show, c[0])), cases))
    for label, r in results:
        if r is None:
            noref += 1
            continue
        total += 1
        ref, m, e = r
        findings += sum(ref.values())
        for k, n in ref.items():
            fired[k[3]] += n
        missing += sum(m.values())
        extra += sum(e.values())
        bad += bool(m or e)
        for k in list(m.elements()) + list(e.elements()):
            by_rule[k[3]] += 1
    print(f'compiles={total} identical={total - bad} differ={bad} noref={noref} '
          f'findings={findings} missing={missing} extra={extra} by_rule={dict(by_rule)}')
    print('reference findings by rule:', dict(sorted(fired.items())))
    silent = sorted(RULES - set(fired))
    if silent:
        print('rules with no reference finding:', ' '.join(silent))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
