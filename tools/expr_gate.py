#!/usr/bin/env python3
"""Expression types of dv-lint's typer (`dv-lint --exprs`) against slang's
(tools/exprdump.cpp), the types rules read as `Expression::type`.

Lines are keyed by (file, line, column, expression kind). slang may print
several types for one key (a body elaborated per instance or
specialization); the typer's type matches when it is one of them. Keys only
one side prints are counted apart: the typer has no type there, or slang
does not walk that code (an unused generic class).

    tools/expr_gate.py [--show=N] --repo UVM_SRC | --dir DIR | --corpus BUILD... | -- ARGS...
"""
import collections, concurrent.futures, glob, os, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
EXPRDUMP = os.environ.get('EXPRDUMP', os.path.join(REPO, 'build', 'uvmcheck_exprdump'))
DVLINT = os.environ.get('DV_LINT', os.path.join(REPO, 'rust', 'target', 'release', 'dv-lint'))


def dump(cmd, env=None):
    out = collections.defaultdict(set)
    r = subprocess.run(cmd, capture_output=True, text=True, errors='replace', env=env)
    for line in r.stdout.splitlines():
        parts = line.split(' ', 5)
        if len(parts) < 6:
            continue
        loc, kind, bits, s, four, ty = parts
        out[(loc, kind)].add((bits, s, four, ty))
    return out


def compare(ref, got):
    """(matched, wrong [(key, ref types, ours)], missing keys, extra keys)."""
    ok = 0
    wrong = []
    for k, ts in ref.items():
        if k in got:
            # Every type the typer gives (two expressions may share a key).
            bad = [t for t in got[k] if t not in ts]
            if bad:
                wrong.append((k, ts, bad[0]))
            else:
                ok += 1
    return ok, wrong, [k for k in ref if k not in got], [k for k in got if k not in ref]


def cases(argv):
    if argv[:1] == ['--repo']:
        uvm = argv[1]
        files = sorted(glob.glob(os.path.join(REPO, 'tests/**/*.sv*'), recursive=True)
                       + glob.glob(os.path.join(REPO, 'examples/**/*.sv*'), recursive=True))
        return [(os.path.relpath(f, REPO), ['+incdir+' + uvm, '+incdir+' + os.path.join(REPO, 'testdata/common'),
                                           '+incdir+' + os.path.dirname(f), f], [], [], None) for f in files]
    if argv[:1] == ['--dir']:
        files = sorted(glob.glob(os.path.join(argv[1], '**/*.sv'), recursive=True))
        return [(os.path.relpath(f, argv[1]), ['+incdir+' + os.path.dirname(f), f], [], [], None) for f in files]
    if argv[:1] == ['--corpus']:
        # The corpus and its tooling live under the gitignored references/.
        sys.path.insert(0, os.path.join(REPO, 'references', 'corpus-tools'))
        import corpus  # noqa: E402
        man = corpus.load()
        out = []
        for b in [b for b in man['builds'] if b['name'] in argv[1:]]:
            args, env = corpus.build(b, man)
            nat = corpus.tool_args(b, 'native', man)
            sl = corpus.tool_args(b, 'slang', man)
            unit = ['--single-unit'] if '--single-unit' in sl else []
            out.append((b['name'], args, [*nat, *unit], sl, {**os.environ, **env}))
        return out
    return [('args', argv[1:] if argv[:1] == ['--'] else argv, [], [], None)]


def run(case):
    label, args, nat, sl, env = case
    with tempfile.TemporaryDirectory() as tmp:
        sargs = args
        if sl or env:
            sys.path.insert(0, os.path.join(REPO, 'references', 'corpus-tools'))
            from corpus_gate import sv_only  # noqa: E402
            sargs = sv_only(args, tmp, env)
        ref = dump([EXPRDUMP, *sl, *sargs], env)
    got = dump([DVLINT, '--exprs', *nat, *args], env)
    return label, compare(ref, got)


def main():
    argv = sys.argv[1:]
    show = 20
    while argv and argv[0].startswith('--show'):
        show = int(argv.pop(0).split('=')[1])
    todo = cases(argv)
    with concurrent.futures.ThreadPoolExecutor(os.cpu_count() or 4) as pool:
        results = list(pool.map(run, todo))
    ok = nwrong = nmiss = nextra = 0
    pattern = collections.Counter()
    sample = {}
    for label, (o, wrong, miss, extra) in results:
        ok += o
        nwrong += len(wrong)
        nmiss += len(miss)
        nextra += len(extra)
        for k, ts, ours in wrong:
            ref = sorted(ts)[0]
            key = (k[1], ' '.join(ref), ' '.join(ours))
            pattern[key] += 1
            sample.setdefault(key, f'{label}: {k[0]}')
    if '--kinds' in os.environ.get('EXPR_GATE', ''):
        mk = collections.Counter(k[1] for _, r in results for k in r[2])
        ek = collections.Counter(k[1] for _, r in results for k in r[3])
        print('missing by kind:', mk.most_common(12))
        print('extra by kind:', ek.most_common(12))
        for side, i in (('missing', 2), ('extra', 3)):
            seen = collections.Counter()
            for label, r in results:
                for k in r[i]:
                    if seen[k[1]] < 3:
                        seen[k[1]] += 1
                        print(f'  {side} {k[1]} {label}: {k[0]}')
    for key, n in pattern.most_common(show):
        print(f'{n:6} {key[0]}: slang "{key[1]}" typer "{key[2]}"  e.g. {sample[key]}')
    print(f'compiles={len(todo)} matched={ok} wrong={nwrong} missing={nmiss} extra={nextra}')
    return 1 if nwrong else 0


if __name__ == '__main__':
    sys.exit(main())
