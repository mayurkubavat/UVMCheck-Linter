# Precision on OpenTitan's DV libraries

OpenTitan's shared DV code (`dv_lib`, `dv_utils`, `csr_utils`, `dv_base_reg`,
`push_pull_agent`, `common_ifs`, `mem_model`, `mem_bkdr_util`, `str_utils`) is a well-reviewed, simulator-clean UVM code base, so a finding
of ERROR severity there is most likely a false positive. The corpus has it
as vendored by ibex (`ibex/vendor/lowrisc_ip/dv`, about 7,900 non-blank
lines), compiled by the `ibex-dv` build with its own file list.

## Run

    C=references/corpus-tools
    eval "$($C/corpus.py env ibex-dv)"
    target/release/dv-lint --no-config --no-cache --preset all --experimental \
        --json ibex.json $($C/corpus.py args ibex-dv)

then the findings whose file is under `vendor/lowrisc_ip/`. Both engines
give the same counts for the rules below (corpus gate, `lint`).

## Result (2026-11-08)

| | Findings | ERROR |
|---|---|---|
| Before | 5,428 | 170 |
| After the first five fixes | 5,263 | 15 |
| After the last four | 5,248 | 0 |

Over all of `ibex-dv`, ERROR findings went from 586 to 226.

### False positives fixed

| Rule | OT findings | Cause | Fix (both engines) |
|---|---|---|---|
| SV-DED-003 AccidentalEmptyConditional | 78 | `` `DV_CHECK `` expands to `if (T_) ; else begin ... end`, a deliberate idiom (an X condition takes the else) | An empty then-branch with an `else` is not reported |
| SV-OOP-011 DowncastWithoutDynamicCast | 39 (142 in ibex-dv) | Names that look like UVM classes (`uvm_` prefix) matched: `uvm_radix_enum'(...)`, `uvm_reg_data_t'(...)` are enum and vector casts | A name a typedef declares is not a class unless a class of that name exists |
| SVA-VAC-001 SvaVacuousAntecedent | 17 | Any `a && !b` was called constant false (`valid && !ready`) | Only a term together with its own negation (`req && !req`) |
| SV-ERR-002 UseAfterFailedNullCheck | 13 | `` `downcast `` / `` `dv_fatal `` guard the fatal with `uvm_report_enabled(..., ID_)` where `ID_` is `$sformatf("%m")` or `` `gfn `` (`get_full_name()`): not recognized as a report guard, so the path was thought to continue | Calls that build the message id do not stop a condition being the report guard |
| UVM-CFG-005 ConfigDbCrossComponentResolver | 8 | `dv_base_agent::CFG_T`, a type parameter, compared as text with the concrete `*_agent_cfg` types set elsewhere | A type declared in the calling class (type parameter, class typedef) is not compared |

### The last 15, fixed next (2026-11-09)

| Rule | OT findings | Cause | Fix (both engines) |
|---|---|---|---|
| SV-LANG-005 NonBlockingInFunction | 8 | `reset_signals()` drives interface signals with `<=`. IEEE 1800 13.4.4 allows nonblocking assignments in functions, and this is the drive SV-INT-001 asks for | Clocking block and virtual interface targets are not reported; the message no longer says the LRM forbids `<=` |
| SV-INT-001 BlockingAssignmentToInterface | 4 | `cfg.vif.if_mode = ...` in `build_phase`: configuration bits written before simulation time starts | Not reported in build, connect, end_of_elaboration and start_of_simulation phases |
| UVM-CFG-001 UvmConfigDbTypeMismatch | 2 | `get` through the type parameter `CFG_T` | A type parameter on either side is not compared (as UVM-CFG-005) |
| UVM-TST-001 CheckerCannotFail | 1 | `dv_base_scoreboard #(RAL_T, CFG_T, COV_T)`, a parameterized base | Parameterized checkers are bases their subclasses fill in |

OpenTitan DV ERROR findings are now 0 (from 170); over all of ibex-dv, 586 -> 207.

### Volume

SV-STY-010 (magic numbers, INFO) is 3,015 of the 5,263 findings, mostly
widths and field sizes in register and CSR code. It is advisory and off by
default; it should stay so.
