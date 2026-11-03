#!/usr/bin/env python3
"""Parity of the Rust parser (sv-syntax) with Slang's parser.

For every .sv file under ROOT/tests (an sv-tests checkout), skipping
:should_fail tests, runs Slang's outline tool and the Rust one and checks:
  - the file and everything it includes round-trips through the tree,
  - when Slang parses the file without errors, so does the Rust parser,
    and both list the same declarations in the same order.

    cmake --build build --target uvmcheck_cst_outline
    cargo build --release -p sv-syntax --example outline   # in rust/
    tools/cst_parity.py ~/sv-tests [chapter-8]
"""
import glob, os, re, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
SLANG = os.environ.get('SLANG_OUTLINE', os.path.join(REPO, 'build', 'uvmcheck_cst_outline'))
RUST = os.environ.get('RUST_OUTLINE', os.path.join(REPO, 'rust', 'target', 'release', 'examples', 'outline'))


def main():
    root = sys.argv[1]
    sel = sys.argv[2] if len(sys.argv) > 2 else ''
    ok = slang_err = skip = 0
    fails = []
    for f in sorted(glob.glob(root + '/tests/**/*.sv', recursive=True)):
        if sel and sel not in f:
            continue
        hdr = open(f, errors='replace').read(4000)
        if ':should_fail' in hdr:
            skip += 1
            continue
        args = []
        m = re.search(r'^:defines:(.*)$', hdr, re.M)
        if m:
            args += ['+define+' + d for d in m.group(1).split()]
        m = re.search(r'^:incdirs:(.*)$', hdr, re.M)
        if m:
            args += ['+incdir+' + os.path.join(root, d) for d in m.group(1).split()]
        args += ['+incdir+' + os.path.dirname(f)]
        try:
            a = subprocess.run([SLANG] + args + [f], capture_output=True, text=True, timeout=20, errors='replace')
            b = subprocess.run([RUST, '--check', '--errors'] + args + [f], capture_output=True, text=True,
                               timeout=20, errors='replace')
        except subprocess.TimeoutExpired:
            fails.append((f, 'timeout'))
            continue
        if 'round-trip failures 0' not in b.stderr:
            fails.append((f, 'round-trip: ' + b.stderr[-300:]))
            continue
        x, y = a.stdout.splitlines(), b.stdout.splitlines()
        if not x or x[-1] != 'errors 0':
            slang_err += 1
            continue
        if x == y:
            ok += 1
            continue
        why = 'rust errors: ' + b.stderr.split('\n')[0] if y and y[-1] != 'errors 0' else ''
        if not why:
            i = next((k for k in range(min(len(x), len(y))) if x[k] != y[k]), min(len(x), len(y)))
            why = f'outline at {i}: slang={x[i:i+2]} rust={y[i:i+2]}'
        fails.append((f, why))
    print(f'ok={ok} fail={len(fails)} slang_errors={slang_err} skipped={skip}')
    for f, why in fails[:60]:
        print(os.path.relpath(f, root), why[:300])


if __name__ == '__main__':
    main()
