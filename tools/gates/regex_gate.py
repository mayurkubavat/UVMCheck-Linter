#!/usr/bin/env python3
"""Regex gate: dv-lint's patterns (crates/dv-lint/src/pattern.rs) against std::regex.

uvmcheck matches waiver messages, file headers and names with std::regex
(ECMAScript). dv-lint matches them with the regex crate behind an ECMAScript
check, which must never change what a pattern means. For each case
(pattern, icase, text) both sides answer regex_search and regex_match, or
refuse the pattern. The gate fails when
  dv-lint loads a pattern and answers differently, or
  dv-lint loads a pattern std::regex refuses.
dv-lint refusing a pattern std::regex accepts is allowed (it fails closed:
the waiver does not load and says why) and is counted, as are cases where
std::regex throws while matching (libc++'s complexity limit). On macOS,
libc++ misreads '\\b' at the end of the input; a '\\b' case where only it
differs, and Python's re (ASCII) agrees with dv-lint, counts as agreed.

The cases: every pattern in the repository's configs, docs and tests and
the default naming patterns, each against texts built from it, and a fuzzed
set from a small grammar (--fuzz N, seeded, default 20000).

    cmake --build build --target uvmcheck_regexcheck
    cargo build --release -p dv-lint --example regexcheck
    tools/gates/regex_gate.py [--fuzz N] [--seed S] [--write-vectors] [--show N]

--write-vectors stores the agreed cases in testdata/regex/vectors.tsv, which
`cargo test` checks with no C++ build. std::regex differs between libc++ and
libstdc++ at the edges; the vectors record the platform that wrote them.
"""
import argparse, os, random, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
ORACLE = os.environ.get('UVMCHECK_REGEXCHECK', os.path.join(REPO, 'build', 'uvmcheck_regexcheck'))
NATIVE = os.environ.get('DV_REGEXCHECK', os.path.join(REPO, 'target', 'release', 'examples', 'regexcheck'))
VECTORS = os.path.join(REPO, 'testdata', 'regex', 'vectors.tsv')


def esc(s):
    out = []
    for ch in s.encode('utf-8'):
        c = chr(ch)
        if c == '\\': out.append('\\\\')
        elif c == '\t': out.append('\\t')
        elif c == '\n': out.append('\\n')
        elif c == '\r': out.append('\\r')
        elif ch < 0x20 or ch >= 0x7f: out.append('\\x%02x' % ch)
        else: out.append(c)
    return ''.join(out)


def run(binary, lines):
    r = subprocess.run([binary], input='\n'.join(lines) + '\n', capture_output=True, text=True, check=True)
    return r.stdout.splitlines()


# Patterns written in the repository: config and waiver files, docs, tests.
PATTERN_KEYS = re.compile(r'"(?:message|pattern|file_header_pattern)"\s*:\s*"((?:[^"\\]|\\.)*)"')
INLINE = re.compile(r'uvmcheck:\s*waive\S*\s+\S+\s+"((?:[^"\\]|\\.)*)"')


def harvested():
    found = set()
    for top in ('docs', 'testdata', 'cpp/tests', 'python', 'crates', 'examples'):
        for root, _, files in os.walk(os.path.join(REPO, top)):
            for f in files:
                if not f.endswith(('.json', '.py', '.md', '.sv', '.svh', '.rs', '.toml')):
                    continue
                try:
                    text = open(os.path.join(root, f), encoding='utf-8', errors='replace').read()
                except OSError:
                    continue
                for m in PATTERN_KEYS.finditer(text):
                    found.add(m.group(1).encode().decode('unicode_escape', errors='replace'))
                for m in INLINE.finditer(text):
                    found.add(m.group(1))
    return sorted(p for p in found if p and len(p) < 200)


# The naming engine's default patterns (cpp/src/style/NamingEngine.cpp).
def naming_defaults():
    src = open(os.path.join(REPO, 'cpp', 'src', 'style', 'NamingEngine.cpp'), encoding='utf-8').read()
    return sorted(set(m.group(1) for m in re.finditer(r'R"\((\^[^)]*(?:\)[^"]*)?)\)"', src)))


ALPHA = 'ab_9A.'
TEXTS = ['', 'a', 'b', 'ab', 'ba', 'aab', 'abab', 'A', 'aB', '_', '9', 'a9_', 'a.b', 'a\nb', 'a\rb', 'a b',
         'abc', 'x', '[', ']', '-', 'uvm_info', 'UVM_LOW', 'my_type_t', 'width 8', '\t', 'é', 'aé']


def fuzz_pattern(rng, depth=0):
    """A pattern from a grammar covering ECMAScript and the crate's own syntax."""
    atoms = ['a', 'b', '_', '9', 'A', '.', r'\w', r'\W', r'\d', r'\s', r'\S', r'\b', r'\B', '^', '$', r'\.',
             r'\-', r'\]', r'\[', '[ab]', '[^a]', '[a-b]', '[a-]', '[-a]', r'[\w.]', '[[:alpha:]]', '[[:digit:]_]',
             r'\x41', r'\t', r'\n', r'\r', ' ', '#', '-', ']', '}', ',',
             # Syntax only one side has, or reads differently:
             r'\A', r'\z', r'\pL', r'\x{41}', r'\u0041', r'\1', r'\0', '[]a]', '[^]', '[[a]', '[a&&b]', '[a--b]',
             r'[\b]', '(?i)', '(?=a)', '(?!a)', '(?<n>a)', 'a{,2}', 'é', '[é]', r'\é', r'\c']
    if depth < 3 and rng.random() < 0.35:
        inner = fuzz_pattern(rng, depth + 1)
        atom = rng.choice(['(%s)', '(?:%s)', '%s|' + fuzz_pattern(rng, depth + 1)]) % inner
    else:
        atom = rng.choice(atoms)
    q = rng.choice(['', '', '', '*', '+', '?', '*?', '+?', '??', '{2}', '{1,}', '{0,2}', '{1,2}?'])
    rest = fuzz_pattern(rng, depth + 1) if depth < 4 and rng.random() < 0.5 else ''
    return atom + q + rest


def cases(fuzz, seed):
    rng = random.Random(seed)
    out = []
    for p in harvested() + naming_defaults():
        for t in TEXTS + [p, p.replace('\\', ''), 'x' + p + 'x']:
            for f in ('', 'i'):
                out.append((p, f, t))
    for _ in range(fuzz):
        p = fuzz_pattern(rng)
        for t in rng.sample(TEXTS, 4) + [''.join(rng.choice(ALPHA) for _ in range(rng.randint(1, 6)))]:
            out.append((p, rng.choice(['', '', 'i']), t))
    return out


def python_answer(p, f, t):
    """Python's re (ASCII) on a case: the tie-break for '\\b', which libc++'s
    std::regex gets wrong at the end of the input. None when re cannot say."""
    if '\r' in t:
        return None
    try:
        r = re.compile(p, re.ASCII | (re.IGNORECASE if f == 'i' else 0))
    except re.error:
        return None
    return f'{int(bool(r.search(t)))} {int(bool(r.fullmatch(t)))}'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--fuzz', type=int, default=20000)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--write-vectors', action='store_true')
    ap.add_argument('--show', type=int, default=20)
    a = ap.parse_args()
    for b, how in ((ORACLE, 'cmake --build build --target uvmcheck_regexcheck'),
                   (NATIVE, 'cargo build --release -p dv-lint --example regexcheck')):
        if not os.access(b, os.X_OK):
            print(f'{b} is not built ({how})', file=sys.stderr)
            return 2
    cs = cases(a.fuzz, a.seed)
    lines = ['\t'.join((esc(p), f, esc(t))) for p, f, t in cs]
    std, dv = run(ORACLE, lines), run(NATIVE, lines)
    differ, loads_bad, refused, agreed, threw, libcxx_b = [], [], set(), [], 0, 0
    for line, (p, f, t), s, d in zip(lines, cs, std, dv):
        if s == 'exc':
            threw += 1  # std::regex gave up matching; dv-lint's time is linear
            continue
        if d == 'err':
            if s != 'err':
                refused.add(p)
            else:
                agreed.append(('\t'.join((esc(p), f, '')), 'err'))  # both refuse it
            continue
        if s == 'err':
            loads_bad.append((p, f, t, s, d))
        elif s != d:
            if r'\b' in p and sys.platform == 'darwin' and python_answer(p, f, t) == d:
                libcxx_b += 1  # libc++'s '\b' bug; ECMAScript (and re) agree with dv-lint
                agreed.append((line, d))
                continue
            differ.append((p, f, t, s, d))
        else:
            agreed.append((line, d))
    patterns = len(set(p for p, _, _ in cs))
    print(f'regex gate: {len(cs)} cases, {patterns} patterns; {len(agreed)} agree, '
          f'{len(refused)} patterns refused by dv-lint (std::regex accepts them), '
          f'{len(differ)} answer differently, {len(loads_bad)} load where std::regex refuses; '
          f'std::regex threw while matching {threw}; libc++ \\b cases settled by Python re {libcxx_b}')
    for title, rows in (('DIFFER', differ), ('LOADS, STD REFUSES', loads_bad)):
        for p, f, t, s, d in rows[:a.show]:
            print(f'{title}  pattern {p!r} flags {f!r} text {t!r}: std {s}, dv-lint {d}')
    if a.write_vectors and not differ and not loads_bad:
        os.makedirs(os.path.dirname(VECTORS), exist_ok=True)
        keep = set(agreed)
        both_refuse = sorted(k for k in keep if k[1] == 'err')
        if len(both_refuse) > 2000:  # a sample is enough to pin what is refused
            keep -= set(both_refuse) - set(random.Random(0).sample(both_refuse, 2000))
        keep = sorted(keep)
        with open(VECTORS, 'w', encoding='utf-8') as out:
            out.write(f'# pattern<TAB>flags<TAB>text<TAB>\"search match\" or \"err\" (refused): std::regex results ({sys.platform}),\n'
                      f'# written by tools/gates/regex_gate.py --write-vectors; checked by cargo test.\n')
            for line, d in keep:
                out.write(f'{line}\t{d}\n')
        print(f'wrote {len(keep)} vectors to {os.path.relpath(VECTORS, REPO)}')
    return 1 if differ or loads_bad else 0


if __name__ == '__main__':
    sys.exit(main())
