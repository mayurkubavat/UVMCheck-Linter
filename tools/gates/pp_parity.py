#!/usr/bin/env python3
"""Token-for-token parity of the Rust preprocessor (sv-preproc) with Slang.

Runs both dumpers over every .sv file under ROOT/tests (an sv-tests
checkout), skipping :should_fail tests, and reports files whose
preprocessed token streams differ. __FILE__ paths are compared by basename,
and Slang's split of based-literal digits (04 c11db6) is merged.

    cmake --build build --target uvmcheck_pp_dump
    cargo build --release -p sv-preproc --example ppdump
    tools/gates/pp_parity.py ~/sv-tests [chapter-22]
"""
import sys, os, re, subprocess, glob
root=sys.argv[1]; sel=sys.argv[2] if len(sys.argv)>2 else ''
HERE=os.path.dirname(os.path.abspath(__file__)); REPO=os.path.dirname(os.path.dirname(HERE))
S=os.environ.get('SLANG_PP', os.path.join(REPO,'build','uvmcheck_pp_dump')); R=os.environ.get('RUST_PP', os.path.join(REPO, 'target','release','examples','ppdump'))
def norm(txt):
    out=[]
    for t in txt.split('\n'):
        if re.fullmatch(r'"[^"]*"',t) and '/' in t and t.rstrip('"').endswith(('.sv','.svh','.v','.vh')): t='"'+os.path.basename(t[1:-1])
        if len(out)>=2 and re.fullmatch(r"'[sS]?[bBoOdDhH]",out[-2]) and re.fullmatch(r"[0-9a-fA-FxXzZ?_]+",out[-1]) and re.fullmatch(r"[0-9a-fA-FxXzZ?_]+",t):
            out[-1]+=t; continue
        out.append(t)
    return out
ok=bad=skip=0; fails=[]
for f in sorted(glob.glob(root+'/tests/**/*.sv',recursive=True)):
    if sel and sel not in f: continue
    hdr=open(f,errors='replace').read(4000)
    if ':should_fail' in hdr: skip+=1; continue
    args=[]
    m=re.search(r'^:defines:(.*)$',hdr,re.M)
    if m: args+=['+define+'+d for d in m.group(1).split()]
    m=re.search(r'^:incdirs:(.*)$',hdr,re.M)
    if m: args+=['+incdir+'+os.path.join(root,d) for d in m.group(1).split()]
    args+=['+incdir+'+os.path.dirname(f)]
    try:
        a=subprocess.run([S]+args+[f],capture_output=True,text=True,timeout=20,errors='replace')
        b=subprocess.run([R]+args+[f],capture_output=True,text=True,timeout=20,errors='replace')
    except subprocess.TimeoutExpired:
        fails.append((f,'timeout')); bad+=1; continue
    if a.returncode!=0 and not a.stdout: skip+=1; continue
    x=norm(a.stdout); y=norm(b.stdout)
    if x==y: ok+=1
    else:
        bad+=1
        i=next((k for k in range(min(len(x),len(y))) if x[k]!=y[k]),min(len(x),len(y)))
        fails.append((f,f'at {i}: slang={x[max(0,i-2):i+3]} ours={y[max(0,i-2):i+3]}'))
print(f'ok={ok} bad={bad} skipped={skip}')
for f,why in fails[:40]: print(os.path.relpath(f,root),why[:300])
