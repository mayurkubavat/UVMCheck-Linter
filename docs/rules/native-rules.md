# Rules only dv-lint has

Most dv-lint rules are ports of uvmcheck's C++ rules and are listed in
[README.md](README.md) (generated from the C++ binary). The rules below have no
C++ twin: `dv-lint --list` marks them `native`, and the parity gates
(`tools/gates/lint_parity.py`) leave them out.

| Rule | Name | Severity | What it flags |
|---|---|---|---|
| `UVM-API-003` | DeprecatedUvmApi | WARNING | Calls to UVM methods, and classes extending UVM classes, that the compiled UVM release keeps only as deprecated API (UVM 1.1d/1.2: what `UVM_NO_DEPRECATED` removes, such as `uvm_component::set_config_int` or `uvm_random_sequence`). Read from the release's API pack in `data/uvm-api/`. |
| `SV-RND-030` | ContradictoryConstraints | ERROR | A rand variable whose hard constraints leave no legal value (`len > 10` on a `bit [3:0]`; `x < 5` with `x inside {[8:9]}`). Bounds in different constraint blocks count together only when no user code calls `constraint_mode()`. |
| `SV-RND-031` | SoftConstraintNeverHolds | WARNING | A `soft` constraint the class's hard constraints always override. |
| `SV-RND-032` | DeadDistItem | WARNING | A `dist` item whose values the hard constraints or the type never allow, so its weight is dropped. |
| `SV-COV-018` | CrossBinCountTooLarge | WARNING | A cross whose bin count, computed as a simulator builds the bins (explicit, `[N]`, `[]`, automatic up to `auto_bin_max`), is above 10,000. |
| `SV-RCE-002` | ClockedBlockingRace | WARNING | A variable written with `=` in one edge-triggered always block and read by another on the same edge: a race (IEEE 1800 4.7). |
| `DPI-PRT-001` | ConflictingDpiPrototypes | ERROR | One C function imported with two different prototypes. |
| `SV-COV-019` | CovergroupNeverSampled | WARNING | A covergroup declared in a module, interface, program or package with no sampling event that nothing instantiates, or whose instances no `.sample()` call anywhere in the design targets (`cg_i.sample()`, `bus.cg_i.sample()`, `cg_q[i].sample(x)`). The class covergroups are SV-COV-001's. Quiet on the corpus, whose module and interface covergroups all have a clocking event. |
| `UVM-TLM-007` | UncoveredAnalysisPort | WARNING | A transaction type a monitor publishes on an analysis port that no coverage collector receives: no class with a covergroup (its own, a base class's or a held object's) extends `uvm_subscriber #(T)` or holds an analysis imp, export, FIFO or get/peek port of `T`. Only reported when the testbench declares covergroups at all; one finding per type, naming every port. On the corpus it finds core-v-verif's agents, whose coverage models receive the monitor transactions but have only TODO covergroups. |

The constraint analyzers only read bounds of one variable against constants
(comparisons, `inside`, `!`, `&&`, `soft`, `dist`) and leave everything else
out, so what they report is certain: if a subset of the constraints has no
solution, neither do all of them.
