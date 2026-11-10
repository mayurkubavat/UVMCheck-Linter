# UVMCheck

Static analysis for SystemVerilog (IEEE 1800) and UVM (IEEE 1800.2)
verification code.

    bin/uvmcheck      the linter
    bin/dv-lsp        language server: outline, go to definition, references, hover, completion
    bin/dv-build      compiler-style errors from the same front end
    bin/dv-map        the component tree of a UVM testbench
    bin/dv-filelist   checks and normalizes VCS/Xcelium file lists
    python/           the Python SDK that project configs and Python rules import

Put `bin/` on your `PATH` and keep `python/` beside it: `uvmcheck` looks for
the SDK there when a project has a `.uvmcheck/config.py` or you pass
`--python-rules` (both need `python3`; set `UVMCHECK_PYTHON` to choose the
interpreter).

    uvmcheck tb/*.sv +incdir+tb --preset methodology
    uvmcheck -f tb.f --json findings.json --sarif findings.sarif
    uvmcheck --init            # write a starter .uvmcheck/config.py for this project
    uvmcheck --explain SV-RND-001

Guide, rule catalog and source: https://github.com/mayurkubavat/svlang
