#!/usr/bin/env python3
"""Semantic-check gate for the native builder (dv-build) on Slang's own unit tests.

Each TEST_CASE in Slang's tests/unittests/ast/*.cpp that compiles one
source text is a small design with a known verdict. The text is run
through dv-build and through uvmcheck (Slang); per test file it reports
how many both accept, both reject, only Slang rejects (a native gap) and
only dv-build rejects (a native false error). Cases that set compilation
options or use several trees are skipped.

    tools/slang_unit_gate.py [--list=gap|false|hang] [Assertion Cover ...]
"""
import collections, concurrent.futures, glob, json, os, re, resource, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
UVMCHECK = os.environ.get('UVMCHECK', os.path.join(REPO, 'build', 'uvmcheck'))
DVBUILD = os.environ.get('DV_BUILD', os.path.join(REPO, 'target', 'release', 'dv-build'))
TESTS = os.environ.get('SLANG_TESTS', os.path.join(REPO, 'build-model', '_deps', 'slang-src', 'tests', 'unittests', 'ast'))

CASE = re.compile(r'TEST_CASE\("((?:[^"\\]|\\.)*)"')
TEXT = re.compile(r'fromText\(R"\((.*?)\)"', re.S)


def cases(path):
    src = open(path, encoding='utf-8', errors='replace').read()
    starts = [(m.start(), m.group(1)) for m in CASE.finditer(src)]
    for i, (at, name) in enumerate(starts):
        body = src[at:starts[i + 1][0] if i + 1 < len(starts) else len(src)]
        texts = TEXT.findall(body)
        if len(texts) != 1 or re.search(r'Options|Bag\b|addSyntaxTree\(.*\n.*addSyntaxTree|fromFile', body):
            continue
        if not re.search(r'\b(module|package|interface|program|class|checker|primitive|config)\b', texts[0]):
            continue
        yield name, texts[0]


def limit():
    resource.setrlimit(resource.RLIMIT_AS, (4 << 30, 4 << 30))


def verdicts(text):
    with tempfile.TemporaryDirectory() as tmp:
        f = os.path.join(tmp, 't.sv')
        open(f, 'w').write(text)
        try:
            nat = subprocess.run([DVBUILD, '--quiet', f], capture_output=True, timeout=30, preexec_fn=limit).returncode != 0
        except subprocess.TimeoutExpired:
            nat = 'hang'
        out = os.path.join(tmp, 'r.json')
        try:
            subprocess.run([UVMCHECK, '--no-config', '--no-cache', '--json', out, f], capture_output=True, timeout=30, preexec_fn=limit)
        except subprocess.TimeoutExpired:
            pass
        try:
            sl = json.load(open(out)).get('compileErrors', 0) > 0
        except (OSError, ValueError):
            sl = True
        return nat, sl


def main():
    sel = [a for a in sys.argv[1:] if not a.startswith('--')]
    show = next((a.split('=', 1)[1] for a in sys.argv[1:] if a.startswith('--list=')), '')
    files = sorted(glob.glob(os.path.join(TESTS, '*Tests.cpp')))
    if sel:
        files = [f for f in files if any(s in os.path.basename(f) for s in sel)]
    jobs = [(os.path.basename(f)[:-9], n, t) for f in files for n, t in cases(f)]
    with concurrent.futures.ThreadPoolExecutor(min(6, os.cpu_count() or 4)) as pool:
        rows = list(pool.map(lambda j: (j[0], j[1], *verdicts(j[2])), jobs))
    t = collections.defaultdict(collections.Counter)
    for area, name, nat, sl in rows:
        t[area]['ok' if not nat and not sl else 'reject' if nat and sl else 'gap' if sl else 'false'] += 1
    print(f'{"area":<14} {"both ok":>8} {"both reject":>12} {"gap":>6} {"false":>6}')
    tot = collections.Counter()
    for area in sorted(t):
        c = t[area]
        tot.update(c)
        print(f'{area:<14} {c["ok"]:>8} {c["reject"]:>12} {c["gap"]:>6} {c["false"]:>6}')
    print(f'{"total":<14} {tot["ok"]:>8} {tot["reject"]:>12} {tot["gap"]:>6} {tot["false"]:>6}')
    for area, name, nat, sl in rows:
        kind = 'hang' if nat == 'hang' else 'gap' if sl and not nat else 'false' if nat and not sl else ''
        if kind and kind == show:
            print(kind.upper(), area, '|', name)


if __name__ == '__main__':
    main()
