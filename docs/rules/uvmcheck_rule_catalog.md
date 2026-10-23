# UVMCheck SystemVerilog & UVM Rule Catalog

This catalog documents the rule pool and methodology checks implemented by **UVMCheck**, adhering to **IEEE 1800 (SystemVerilog)**, **IEEE 1800.2 (UVM)**, and industry-standard verification methodology guidelines. This serves as our master specification for porting and expanding rules inside `uvmcheck`.

---

## Summary of Rule Categories

| Category | Description | Primary Inspection Mechanism |
| :--- | :--- | :--- |
| **UVM Methodology** | Factory, phasing, objections, config_db, TLM, sequences | Slang Typed AST + UvmModel DAG |
| **SystemVerilog OOP** | Polymorphism, virtual methods, casting, inheritance | Slang Typed AST Symbol Table |
| **Performance** | Memory duplication, loop delays, solver bottlenecks | AST Statement & Call Flow |
| **Language Pitfalls** | Delta-cycle races, thread leaks, automatic variables | AST Concurrency & Scoping |
| **Dead Code** | Unused variables, uncalled routines, dead parameters | Symbol Reference Analysis |
| **Coverage & SVA** | Covergroup naming, empty bins, assertion clocking | AST & CST Syntax Visitors |
| **Style & Formatting** | Identifier naming regex, indentation, line lengths | Slang CST (SyntaxVisitor) & Tokens |

---

## Category 1: UVM Methodology Compliance (`UVM.*`)

### 1.1 Factory Registration and Object Creation
* **`UVM-FAC-001` (Standard Ref: `SVTB.UVM.FACTORY.NEW`)**
  * **Title:** Direct Instantiation of UVM Component via `new()`
  * **Severity:** ERROR
  * **Description:** Components derived from `uvm_component` must be created through the factory using `type_id::create()` rather than direct `new()` calls. Direct `new()` prevents testcases from overriding components.
  * **Bad:** `drv = new("drv", this);`
  * **Good:** `drv = my_driver::type_id::create("drv", this);`
  * **uvmcheck Status:** **Implemented**

* **`UVM-FAC-002` (Standard Ref: `SVTB.UVM.FACTORY.MACRO_MISMATCH`)**
  * **Title:** Mismatched Factory Registration Macro
  * **Severity:** ERROR
  * **Description:** Classes derived from `uvm_component` must use `` `uvm_component_utils(T) ``, and classes derived from `uvm_object` must use `` `uvm_object_utils(T) ``. Swapping them corrupts factory lookup tables.
  * **Bad:** Class `my_env extends uvm_env` using `` `uvm_object_utils(my_env) ``.
  * **Good:** `` `uvm_component_utils(my_env) ``
  * **uvmcheck Status:** Planned

* **`UVM-FAC-003` (Standard Ref: `SVTB.UVM.FACTORY.MISSING_UTILS`)**
  * **Title:** Missing UVM Factory Registration Macro
  * **Severity:** WARNING
  * **Description:** Concrete classes derived from `uvm_object` or `uvm_component` should be registered with the factory using the appropriate utility macro unless explicitly abstract.
  * **uvmcheck Status:** Planned

* **`UVM-FAC-004` (Standard Ref: `SVTB.UVM.FACTORY.PARAM_UTILS`)**
  * **Title:** Parameterized Class Factory Registration
  * **Severity:** ERROR
  * **Description:** Parameterized UVM components and objects must use `` `uvm_component_param_utils(T) `` or `` `uvm_object_param_utils(T) `` instead of standard utils macros.
  * **uvmcheck Status:** Planned

---

### 1.2 Phasing and Lifecycle Execution
* **`UVM-PHS-001` (Standard Ref: `SVTB.UVM.PHASE.MISSING_SUPER`)**
  * **Title:** Missing `super.<phase>(phase)` Call
  * **Severity:** WARNING
  * **Description:** Overridden standard phases in `uvm_component` (`build_phase`, `connect_phase`, `end_of_elaboration_phase`, etc.) must call `super.<phase>(phase)` so base-class configurations and auto-configurations execute.
  * **Bad:**
    ```systemverilog
    function void build_phase(uvm_phase phase);
        // missing super.build_phase(phase);
    endfunction
    ```
  * **Good:**
    ```systemverilog
    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
    endfunction
    ```
  * **uvmcheck Status:** **Implemented**

* **`UVM-PHS-002` (Standard Ref: `SVTB.UVM.PHASE.TIME_IN_FUNC`)**
  * **Title:** Time-Consuming Statement in Function Phase
  * **Severity:** ERROR
  * **Description:** Function-based phases (`build_phase`, `connect_phase`, `extract_phase`, `check_phase`, `report_phase`, `final_phase`) must execute in zero simulation time. Delay `#`, clock `@`, and `wait` statements are prohibited.
  * **uvmcheck Status:** **Implemented**

* **`UVM-PHS-003` (Standard Ref: `SVTB.UVM.PHASE.TASK_SIG_MISMATCH`)**
  * **Title:** Phase Method Signature Mismatch
  * **Severity:** ERROR
  * **Description:** Standard UVM phases must match the expected prototype (`function void <phase>(uvm_phase phase)` or `task run_phase(uvm_phase phase)`). Declaring `build_phase` as a task or without `uvm_phase phase` causes UVM to silently ignore the phase.
  * **uvmcheck Status:** Planned

---

### 1.3 Objection Management
* **`UVM-OBJ-001` (Standard Ref: `SVTB.UVM.OBJECTION.UNPAIRED_RAISE_DROP`)**
  * **Title:** Unpaired `raise_objection` / `drop_objection`
  * **Severity:** ERROR
  * **Description:** Every `phase.raise_objection()` must have a corresponding, guaranteed `phase.drop_objection()` along all execution paths. Missing drops cause simulation to hang.
  * **uvmcheck Status:** Planned (Requires Control-Flow Graph analysis)

* **`UVM-OBJ-002` (Standard Ref: `SVTB.UVM.OBJECTION.RAISE_IN_LOOP`)**
  * **Title:** Raising Objection Inside a Loop
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Raising and dropping objections inside high-frequency transaction loops incurs severe scheduler overhead. Objections should be raised once outside the transaction loop.
  * **uvmcheck Status:** Planned

* **`UVM-OBJ-003` (Standard Ref: `SVTB.UVM.OBJECTION.TEST_MISSING_OBJECTION`)**
  * **Title:** Top Test Run Phase Lacks Objections
  * **Severity:** WARNING
  * **Description:** Top-level `uvm_test::run_phase` should raise an objection before starting background sequences to prevent simulation from terminating at time 0.
  * **uvmcheck Status:** Planned

---

### 1.4 Configuration Database (`uvm_config_db`)
* **`UVM-CFG-001` (Standard Ref: `SVTB.UVM.CONFIG_DB.TYPE_MISMATCH`)**
  * **Title:** Inconsistent `uvm_config_db` Type Parameter
  * **Severity:** WARNING
  * **Description:** The type parameter `T` in `uvm_config_db#(T)::set(...)` must match the type parameter in the corresponding `uvm_config_db#(T)::get(...)`. Mismatches cause silent lookup failures.
  * **uvmcheck Status:** Planned

* **`UVM-CFG-002` (Standard Ref: `SVTB.UVM.CONFIG_DB.NULL_CONTEXT`)**
  * **Title:** Null Context in Component `config_db` Call
  * **Severity:** WARNING
  * **Description:** Inside a `uvm_component`, calling `uvm_config_db#(T)::get(null, ...)` breaks hierarchical scoping. Use `this` as the context handle.
  * **uvmcheck Status:** Planned

* **`UVM-CFG-003` (Standard Ref: `SVTB.UVM.CONFIG_DB.UNCHECKED_GET`)**
  * **Title:** Unchecked Return Value on `config_db::get`
  * **Severity:** WARNING
  * **Description:** `uvm_config_db#(T)::get(...)` returns a boolean indicating whether the configuration was found. Critical items (like virtual interfaces) must check the return value and issue `uvm_fatal` if missing.
  * **uvmcheck Status:** Planned

---

### 1.5 TLM Connections and Sequences
* **`UVM-TLM-001` (Standard Ref: `SVTB.UVM.TLM.UNBOUND_PORT`)**
  * **Title:** Unconnected TLM Port in Connect Phase
  * **Severity:** ERROR
  * **Description:** Mandatory TLM ports (such as `uvm_blocking_put_port`) left unconnected after `end_of_elaboration_phase` result in runtime fatal errors when called.
  * **uvmcheck Status:** Planned

* **`UVM-SEQ-001` (Standard Ref: `SVTB.UVM.SEQ.DIRECT_VIF_ACCESS`)**
  * **Title:** Sequence Directly Accessing Physical Interface
  * **Severity:** ARCHITECTURE / WARNING
  * **Description:** Sequences should only create and randomize transactions; they should not contain hierarchical references to physical DUT interfaces (`vif.sig <= 1`). Signal driving belongs exclusively to the driver.
  * **uvmcheck Status:** Planned

* **`UVM-SEQ-002` (Standard Ref: `SVTB.UVM.SEQ.START_ITEM_HANDSHAKE`)**
  * **Title:** Unpaired `start_item` and `finish_item`
  * **Severity:** ERROR
  * **Description:** Calling `start_item(req)` without a subsequent `finish_item(req)` locks the sequencer queue and starves drivers.
  * **uvmcheck Status:** Planned

---

## Category 2: SystemVerilog OOP & Semantics (`OOP.*` / `SEMANTIC.*`)

* **`SV-OOP-001` (Standard Ref: `SVTB.OOP.NON_VIRTUAL_OVERRIDE`)**
  * **Title:** Overriding Non-Virtual Method in Base Class
  * **Severity:** WARNING
  * **Description:** Overriding a method that was declared non-virtual in a base class breaks polymorphism. Calls through base-class handles will execute the base implementation rather than the derived one.
  * **uvmcheck Status:** **Implemented**

* **`SV-OOP-002` (Standard Ref: `SVTB.OOP.UNCHECKED_DYNAMIC_CAST`)**
  * **Title:** Unchecked Return Value of `$cast(dest, src)`
  * **Severity:** WARNING
  * **Description:** When `$cast` is called as a function (or cast to `void'`), failing casts leave `dest` as `null` without stopping execution, triggering null-pointer dereference crashes downstream.
  * **Bad:** `void'($cast(tx, item));`
  * **Good:** `if (!$cast(tx, item)) \`uvm_fatal("CASTERR", "Failed cast");`
  * **uvmcheck Status:** Planned

* **`SV-OOP-003` (Standard Ref: `SVTB.OOP.MISSING_SUPER_NEW`)**
  * **Title:** Missing `super.new()` in Class Constructor
  * **Severity:** WARNING
  * **Description:** Derived class constructors should explicitly call `super.new(...)` with appropriate arguments to guarantee base class fields and parent identifiers are properly initialized.
  * **uvmcheck Status:** Planned

* **`SV-OOP-004` (Standard Ref: `SVTB.OOP.PROTECTED_ACCESS_VIOLATION`)**
  * **Title:** Illegal Direct Access to `local` or `protected` Members
  * **Severity:** ERROR
  * **Description:** Flags illegal attempts to bypass object encapsulation.
  * **uvmcheck Status:** Planned

---

## Category 3: Simulation Performance Optimization (`PERF.*`)

* **`SV-PRF-001` (Standard Ref: `SVTB.PERF.PASS_ARRAY_BY_VALUE`)**
  * **Title:** Passing Large Array or Struct by Value
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Passing dynamic arrays, queues, associative arrays, or large structs into tasks/functions by value triggers expensive deep-memory copies on every invocation. Use `const ref` or `ref` instead.
  * **Bad:** `function void process_packet(byte payload[]);`
  * **Good:** `function void process_packet(const ref byte payload[]);`
  * **uvmcheck Status:** Planned

* **`SV-PRF-002` (Standard Ref: `SVTB.PERF.STRING_CONCAT_IN_LOOP`)**
  * **Title:** Heavy String Formatting in High-Speed Loops
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Calling `$sformatf` inside tight simulation loops when messages are filtered by verbosity anyway incurs high memory allocation penalties.
  * **uvmcheck Status:** Planned

* **`SV-PRF-003` (Standard Ref: `SVTB.PERF.INEFFICIENT_CONSTRAINT_SOLVER`)**
  * **Title:** Inefficient Large `inside` Range in Constraint
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Constraints with unbounded sets or deeply nested implications stress BDD/SAT solver engines.
  * **uvmcheck Status:** Planned

---

## Category 4: Language Pitfalls & Thread Concurrency (`PITFALL.*`)

* **`SV-THR-001` (Standard Ref: `SVTB.THREAD.FORK_LOOP_VAR`)**
  * **Title:** Loop Variable Shared by Reference in `fork..join_none`
  * **Severity:** ERROR
  * **Description:** Spawning threads inside a `for` loop with `fork..join_none` without creating an automatic local copy of the loop index causes all threads to see the final post-loop index value.
  * **Bad:**
    ```systemverilog
    for (int i = 0; i < 4; i++) begin
        fork
            drive_port(i); // All 4 threads execute with i == 4!
        join_none
    end
    ```
  * **Good:**
    ```systemverilog
    for (int i = 0; i < 4; i++) begin
        automatic int idx = i;
        fork
            drive_port(idx);
        join_none
    end
    ```
  * **uvmcheck Status:** Planned

* **`SV-THR-002` (Standard Ref: `SVTB.THREAD.NAKED_FORK_WITHOUT_DISABLE`)**
  * **Title:** Background Thread Leak in Task
  * **Severity:** WARNING
  * **Description:** Tasks that launch `fork..join_any` or `fork..join_none` without `disable fork` leave orphan background processes running after the enclosing task exits.
  * **uvmcheck Status:** Planned

---

## Category 5: Dead Code & Redundancy (`DEADCODE.*`)

* **`SV-DED-001` (Standard Ref: `SVTB.DEADCODE.UNUSED_VARIABLE`)**
  * **Title:** Unreferenced Local Variable
  * **Severity:** INFO / WARNING
  * **Description:** Variables declared inside tasks, functions, or class scopes that are never read or written.
  * **uvmcheck Status:** Planned

* **`SV-DED-002` (Standard Ref: `SVTB.DEADCODE.UNCALLED_ROUTINE`)**
  * **Title:** Uncalled Private Function or Task
  * **Severity:** INFO
  * **Description:** Local methods with `local` visibility that are never referenced within the class scope.
  * **uvmcheck Status:** Planned

---

## Category 6: Functional Coverage & Assertions (`COVERAGE.*` / `SVA.*`)

* **`SV-COV-001` (Standard Ref: `SVTB.COV.EMPTY_BINS`)**
  * **Title:** Coverpoint Lacks Explicit Bins
  * **Severity:** WARNING
  * **Description:** A coverpoint declared on a wide integer (e.g. 32-bit int) without explicit bins defaults to 64 auto-bins, resulting in misleading coverage statistics.
  * **uvmcheck Status:** Planned

* **`SV-SVA-001` (Standard Ref: `SVTB.SVA.CONCURRENT_IN_CLASS`)**
  * **Title:** Concurrent Assertion in Dynamic Scope
  * **Severity:** ERROR
  * **Description:** Concurrent assertions (`assert property`) require static clocking domains and cannot be placed inside dynamic class objects.
  * **uvmcheck Status:** Planned

---

## Category 7: Naming Conventions & Style (`STYLE.*`)

* **`SV-STY-001` (Standard Ref: `SVTB.STYLE.CLASS_MEMBER_PREFIX`)**
  * **Title:** Class Property Missing Prefix
  * **Severity:** STYLE
  * **Description:** Enforces that member properties of classes begin with `m_` (e.g. `m_cfg`, `m_pkt_count`) to distinguish local state from parameters and arguments.
  * **uvmcheck Status:** Planned

* **`SV-STY-002` (Standard Ref: `SVTB.STYLE.MACRO_SEMICOLON`)**
  * **Title:** Unnecessary Semicolon After Macro Call
  * **Severity:** STYLE / WARNING
  * **Description:** Macro calls like `` `uvm_component_utils(...) `` or `` `uvm_info(...) `` followed by `;` create empty null statements in declarative scopes.
  * **uvmcheck Status:** Planned

---

## Category 8: Software Engineering Rules (`SV-BUG`, `SV-ERR`, `SV-DSN`, `SV-MET`, `SV-RDB`, `UVM-ARC`, `UVM-TST` ...)

Rules adapted from mature software linters (SonarQube, PMD, clang-tidy, Pylint, SpotBugs, Checkstyle) and from Chidamber-Kemerer coupling and cohesion metrics, tuned for UVM roles: env, test, config object and register model classes get their own exemptions or limits. Shared support lives in `SweSupport.h` (role detection, report-macro handling), `CodeMetrics.h` (per-routine and per-class metrics, computed once per compilation) and `PathWalker.h` (path-sensitive walks for SV-ERR-002 and SV-RSC-001).

Thresholds are read through `swe::ruleOption(rule, key, default)`, so the Python project config can override them per rule; the option names are listed with each rule.

### 8.1 Bug patterns

* **`SV-BUG-001`** (IdenticalBranches)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags if/else and case branches whose bodies are token-identical to an earlier branch of the same statement (clang-tidy bugprone-branch-clone, Sonar S1871).
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-002`** (RepeatedElseIfCondition)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags a condition that repeats an earlier condition of the same if/else-if chain, which makes the later branch dead (Sonar S1862).
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-003`** (SelfAssignment)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags 'x = x' assignments, typically a constructor or setter whose argument shadows the property it was meant to set (clang-tidy bugprone-self-assignment, Sonar S1656).
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-004`** (MismatchedFieldInCopyCompare)
  * **Severity:** ERROR · **Category:** Correctness
  * **Description:** In do_copy/do_compare and similar field-wise methods, flags a field of this object paired with a differently named field of the other object when the matching field exists.
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-005`** (IntegerDivisionToReal)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags integer divisions whose result is used as a real value, which truncates before the conversion (Sonar S2184, SpotBugs ICAST_IDIV_CAST_TO_DOUBLE).
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-006`** (RealEquality)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags == and != on real or shortreal operands, except against an exact zero (Sonar S1244, clang-tidy float equality).
  * **uvmcheck Status:** **Implemented**

* **`SV-BUG-007`** (SuspiciousArgumentOrder)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags positional calls where an argument's name matches a different parameter of the callee, such as write(len, addr) for write(addr, len) (clang-tidy readability-suspicious-call-argument, Sonar S2234).
  * **uvmcheck Status:** **Implemented**

### 8.2 Error handling and self-checking

* **`SV-ERR-001`** (FailureNotReported)
  * **Severity:** ERROR · **Category:** Correctness
  * **Description:** Flags randomize(), $cast(), uvm_config_db::get(), try_* and compare() status checks whose failure branch is empty or only reports below error severity (Sonar S108/S1166).
  * **uvmcheck Status:** **Implemented**

* **`SV-ERR-002`** (UseAfterFailedNullCheck)
  * **Severity:** ERROR · **Category:** Correctness
  * **Description:** Flags a handle that a null check, a failed uvm_config_db::get() or a failed $cast() showed to be null, dereferenced later on the same path because the failure branch did not stop (SpotBugs NP_NULL_ON_SOME_PATH).
  * **uvmcheck Status:** **Implemented**

* **`SV-ERR-003`** (ProcessTerminationInClass)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags $finish, $stop and $exit called from class methods. The run then skips the extract, check and report phases and the UVM report summary (Sonar S1147).
  * **uvmcheck Status:** **Implemented**

* **`UVM-REP-006`** (BlanketErrorDemotion)
  * **Severity:** ERROR · **Category:** UVM
  * **Description:** Flags report catchers that demote or swallow errors without filtering on ID or message, and severity overrides/actions that disable UVM_ERROR or UVM_FATAL for every ID (Pylint W0718 broad-exception-caught).
  * **uvmcheck Status:** **Implemented**

* **`UVM-TST-001`** (CheckerCannotFail)
  * **Severity:** ERROR · **Category:** UVM
  * **Description:** Flags scoreboard and checker classes with no way to report a failure: no `uvm_error or `uvm_fatal, no assertion and no comparator, in the class, its bases or the helpers it calls (Sonar S2699, tests without assertions).
  * **uvmcheck Status:** **Implemented**

* **`UVM-TST-002`** (FixedDelayBeforeDrop)
  * **Severity:** WARNING · **Category:** UVM
  * **Description:** Flags a constant #delay immediately before drop_objection(), used to let traffic drain (xUnit 'sleepy test', Sonar S2925).
  * **uvmcheck Status:** **Implemented**

### 8.3 Architecture and design

* **`UVM-ARC-001`** (LayerDependency)
  * **Severity:** WARNING · **Category:** Architecture
  * **Description:** Flags classes that depend on a higher testbench layer (item < sequence < agent < env < test), such as a sequence that references a scoreboard or an agent that references the env (ArchUnit layer rules).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ARC-002`** (HierarchyReachThrough)
  * **Severity:** WARNING · **Category:** Architecture
  * **Description:** Flags expressions that reach through three or more component handles (env.agt.drv.vif), outside connect_phase wiring (PMD LawOfDemeter, Fowler 'message chains').
  * **uvmcheck Status:** **Implemented**

* **`UVM-ARC-003`** (FileIncludedInMultiplePackages)
  * **Severity:** ERROR · **Category:** Architecture
  * **Description:** Flags a file whose class definitions are `include'd into more than one package. Each package gets its own, incompatible type with the same name (C++ one-definition rule).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ARC-004`** (StringHierarchyLookup)
  * **Severity:** WARNING · **Category:** Architecture
  * **Description:** Flags uvm_root/uvm_component find(), find_all() and lookup() calls outside tests and end_of_elaboration_phase. Looking components up by path string is a service locator that hides dependencies and breaks silently when the hierarchy changes.
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-001`** (TypeSwitchByName)
  * **Severity:** WARNING · **Category:** Architecture
  * **Description:** Flags branching on get_type_name() compared with string literals instead of virtual dispatch (Sonar 'instanceof chains', open/closed principle).
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-002`** (GodClass)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags classes that are very complex (WMC >= 47), use many other classes' data (ATFD > 5) and have low cohesion (TCC < 1/3): PMD's GodClass detection strategy.
  * **Options:** `wmc` (default 47), `atfd` (default 5), `tcc_percent` (default 33)
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-003`** (LowCohesion)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags classes whose methods split into two or more groups that share no fields and do not call each other (LCOM4 > 1): the class is really several classes.
  * **Options:** `groups` (default 2), `min_group_size` (default 2)
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-004`** (HighCoupling)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags classes other than envs and tests that reference more than 10 distinct user class types (CK coupling between objects, PMD CouplingBetweenObjects).
  * **Options:** `max_cbo` (default 10)
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-005`** (DeepInheritance)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags classes with more than 3 user-defined ancestors above the UVM base class (CK depth of inheritance tree, Pylint too-many-ancestors).
  * **Options:** `max_depth` (default 3)
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-006`** (OverrideOnlyCallsSuper)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags overrides whose whole body is the super call with the same arguments, such as a connect_phase that only calls super.connect_phase(phase) (Pylint useless-parent-delegation, Sonar S1185). Constructors are kept.
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-007`** (EmptyOverrideOfConcrete)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags an empty override of a user base-class method that has a body, which silently switches the inherited behavior off (Fowler 'refused bequest', Liskov substitution).
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-008`** (MutablePackageState)
  * **Severity:** WARNING · **Category:** Architecture
  * **Description:** Flags non-const variables declared in packages. They are shared by every env instance and test in a run (clang-tidy cppcoreguidelines-avoid-non-const-global-variables).
  * **uvmcheck Status:** **Implemented**

* **`UVM-CFG-006`** (PlusargsOutsideTest)
  * **Severity:** WARNING · **Category:** UVM
  * **Description:** Flags $test$plusargs, $value$plusargs and uvm_cmdline_processor reads in classes other than tests and config objects. Reading the command line deep inside components is hidden global configuration.
  * **uvmcheck Status:** **Implemented**

### 8.4 Size and complexity metrics

* **`SV-MET-001`** (CognitiveComplexity)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags routines whose Sonar cognitive complexity (nesting-weighted branching, with fork/join counted as nesting) exceeds the threshold (default 25; Sonar S3776).
  * **Options:** `threshold` (default 25)
  * **uvmcheck Status:** **Implemented**

* **`SV-MET-002`** (LongRoutine)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags routines with more statements than the threshold (default 100; clang-tidy readability-function-size, Checkstyle MethodLength).
  * **Options:** `max_statements` (default 100)
  * **uvmcheck Status:** **Implemented**

* **`SV-MET-003`** (TooManyArguments)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags tasks and functions with more formal arguments than the threshold (default 7; Pylint R0913, Sonar S107). Overrides are judged at the base declaration only.
  * **Options:** `max_args` (default 7)
  * **uvmcheck Status:** **Implemented**

* **`SV-MET-004`** (DeepNesting)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags routines whose control structures nest deeper than the threshold (default 5; Checkstyle NestedIfDepth, clang-tidy readability-function-size NestingThreshold).
  * **Options:** `max_depth` (default 5)
  * **uvmcheck Status:** **Implemented**

* **`SV-MET-005`** (ComplexCondition)
  * **Severity:** WARNING · **Category:** Maintainability
  * **Description:** Flags conditions with more && / || operators than the threshold (default 5; Pylint R0916, Checkstyle BooleanExpressionComplexity).
  * **Options:** `max_bool_ops` (default 5)
  * **uvmcheck Status:** **Implemented**

* **`SV-MET-006`** (LargeClass)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags classes with more than 30 properties or 30 methods; config objects and sequence items may have 80 properties, register models are exempt (PMD TooManyFields and TooManyMethods, Pylint R0902/R0904).
  * **Options:** `max_fields` (default 30), `max_data_fields` (default 80), `max_methods` (default 30)
  * **uvmcheck Status:** **Implemented**

### 8.5 API hygiene and readability

* **`SV-API-001`** (RefArgumentNeverWritten)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags ref arguments that the routine only reads; declare them const ref so callers know they are not modified (clang-tidy readability-non-const-parameter). Virtual methods are skipped because overrides share the signature.
  * **uvmcheck Status:** **Implemented**

* **`SV-DED-013`** (UnusedRoutineArgument)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags task and function arguments that the body never uses (clang-tidy misc-unused-parameters, Pylint W0613). Virtual methods and overrides, phase methods, DPI routines and empty bodies are skipped.
  * **uvmcheck Status:** **Implemented**

* **`SV-RDB-001`** (PositionalFlagLiterals)
  * **Severity:** INFO · **Category:** Maintainability
  * **Description:** Flags calls that pass two or more 0/1 literals positionally to 1-bit arguments; use named argument binding (clang-tidy bugprone-argument-comment, Clean Code flag arguments).
  * **Options:** `min_flags` (default 2)
  * **uvmcheck Status:** **Implemented**

### 8.6 Resources and concurrency

* **`SV-RSC-001`** (LockNotReleased)
  * **Severity:** ERROR · **Category:** Concurrency
  * **Description:** Flags semaphore get()/try_get() and sequencer lock()/grab() that some path leaves without the matching put()/unlock()/ungrab(), such as an early return (Sonar S2222).
  * **uvmcheck Status:** **Implemented**

* **`SV-RSC-002`** (FileHandleNotClosed)
  * **Severity:** WARNING · **Category:** Correctness
  * **Description:** Flags $fopen calls in classes that never call $fclose, which loses buffered trace data and exhausts file handles in long regressions (Sonar S2095).
  * **uvmcheck Status:** **Implemented**

* **`SV-CNC-003`** (PollingLoop)
  * **Severity:** WARNING · **Category:** Concurrency
  * **Description:** Flags loops that test a flag or signal and wait a constant #delay between tests, instead of wait(), @ or an event (busy-wait smell, Sonar S2925).
  * **uvmcheck Status:** **Implemented**

* **`SV-CNC-004`** (SharedWriteFromForks)
  * **Severity:** WARNING · **Category:** Concurrency
  * **Description:** Flags class properties written in two or more branches of one fork, or in a join_none thread and in its parent, without a semaphore (SpotBugs IS2_INCONSISTENT_SYNC).
  * **uvmcheck Status:** **Implemented**

* **`SV-THR-003`** (UnboundedThreadSpawn)
  * **Severity:** WARNING · **Category:** Concurrency
  * **Description:** Flags fork ... join_none inside forever or while loops with no wait fork, disable fork or semaphore get in the loop to bound outstanding threads (thread-leak checks).
  * **uvmcheck Status:** **Implemented**

### 8.7 Advisory (opt-in: `--preset advisory` or `--preset all`)

* **`SV-DSN-009`** (RepeatedEnumSwitch)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags an enum that is switched on with case in three or more classes, a sign that behavior per kind belongs in a polymorphic item or policy class (Fowler 'repeated switches'). Advisory: off by default.
  * **Options:** `min_classes` (default 3)
  * **uvmcheck Status:** **Implemented**

* **`SV-DSN-010`** (QueryChangesState)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags functions named get_*, is_* or has_* that write class properties (command-query separation). get_next_* is allowed. Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-RDB-002`** (MagicNumber)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags numeric literals other than 0, 1 and -1 in class method bodies: conditions, loop bounds and sizes; delays are left to the magic-delay rules (Checkstyle MagicNumber, Sonar S109). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-RDB-003`** (CommentedOutCode)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags blocks of // comments that parse as SystemVerilog statements or declarations (Sonar S125). Advisory: off by default.
  * **Options:** `min_lines` (default 2)
  * **uvmcheck Status:** **Implemented**

* **`SV-RDB-004`** (UndocumentedExtensionPoint)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags new virtual methods without a preceding comment in classes meant to be extended: classes with user subclasses, classes named *base*, and callbacks (Checkstyle MissingJavadocMethod, Pylint C0116). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-RDB-005`** (RedundantBoolean)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags 1-bit values compared with 1'b1/1'b0 and 'if (c) return 1; else return 0;' (Sonar S1125, clang-tidy readability-simplify-boolean-expr). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-TST-001`** (ConstantReseed)
  * **Severity:** INFO · **Category:** Advisory
  * **Description:** Flags srandom() or set_randstate() with a constant in components, sequences and other non-test classes; every seed of the regression then runs the same stimulus. Advisory: off by default.
  * **uvmcheck Status:** **Implemented**
