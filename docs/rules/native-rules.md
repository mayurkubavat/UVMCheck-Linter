# Rules only the native engine has

Most rules exist in both engines. The rules below are in the native engine
only: the catalog ([README.md](README.md)) marks them **Native**,
`uvmcheck --list` prints `native` after them, and the parity gates
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

## Rules the native engine answers from declarations

Some rules both engines have must know what a reference is. The native engine
resolves it; the C++ engine goes by how the handle is named.

| Rule | Native engine | C++ engine |
|---|---|---|
| `SV-INT-001`, `SV-LANG-005` | An assignment target is an interface drive when its declarations say so: `cfg.bus.drv.data` with `bus` declared `virtual bus_if` (also through a typedef, an inherited property, or an array of handles) and `drv` a clocking block of `bus_if`. A struct called `my_vif` is not one. | A target whose text has `vif.`, `_vif.` or `.cb.`, or starts with `cb.` outside a class. |
| `SV-DRV-001` | A task drives an interface when one of its `<=` targets is such a signal by declaration. | A task whose text has `vif.` or `intf.` and `<=`. |
| `UVM-VIF-DIR` | A monitor drives an interface when the assignment target is reached through a virtual interface handle, whatever it is called. | A target whose text has `vif.`, `intf.` or `_vif.`. |
| `SV-OOP-008` | A class member is a non-virtual interface when its type is a declared `interface`; an `interface class` called `printable_if` is not one. | A type whose name ends in `_if` or `_vif`. |

A target or type the native engine cannot resolve (the handle's type is a type
parameter, the path has a call in it, a class or interface is declared twice
or is not compiled) is judged by its names, as in the C++ engine, so the two
agree on those.

## Objection pairing

`UVM-OBJ-001`, `UVM-OBJ-003` and `UVM-PHS-006` each judged one routine alone,
so the usual ways of holding an objection across routines were reported as
defects, up to five ERRORs for a `pre_body`/`post_body` pair. The native
engine answers all three from one analysis (`dv_lint::sem::objections`):

| Code | Native engine | C++ engine |
|---|---|---|
| A raise in one routine, the drop in another routine of the class, a base class or a derived class (`pre_body`/`post_body`, `pre_start`/`post_start`, helper tasks) | Paired: no finding. A routine that raises and drops equally often is balanced in itself and does not pair with another one. | Each routine reported. |
| `if (c) raise` ... `if (c) drop`, the same condition text | Balanced. | `UVM-OBJ-003` asymmetric branch, twice. |
| `if (h != null) h.raise_objection(this)` with no `else` | Held under that condition; not an asymmetric branch. | `UVM-OBJ-003` asymmetric branch. |
| Two objections in one routine (`phase` and another handle) | Each handle is followed on its own. | One state for all. |
| A `return` inside a loop while the objection is held, a drop of nothing inside a loop | `UVM-OBJ-003` reports it; a loop that raises and drops in each round is balanced. | Loop bodies are not looked at. |

A raise nobody in the class family drops, a drop of something never raised, a
`return` while the routine's own objection is held, and a drop in only one
branch are reported by both engines. On uvm-core the three rules gave 8 ERROR
findings, all in code that is correct; the native engine gives none.
