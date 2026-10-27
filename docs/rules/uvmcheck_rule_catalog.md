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
* **`UVM-FAC-001`**
  * **Title:** Direct Instantiation of UVM Component via `new()`
  * **Severity:** ERROR
  * **Description:** Components derived from `uvm_component` must be created through the factory using `type_id::create()` rather than direct `new()` calls. Direct `new()` prevents testcases from overriding components.
  * **Bad:** `drv = new("drv", this);`
  * **Good:** `drv = my_driver::type_id::create("drv", this);`
  * **uvmcheck Status:** **Implemented**

* **`UVM-FAC-002`**
  * **Title:** Mismatched Factory Registration Macro
  * **Severity:** ERROR
  * **Description:** Classes derived from `uvm_component` must use `` `uvm_component_utils(T) ``, and classes derived from `uvm_object` must use `` `uvm_object_utils(T) ``. Swapping them corrupts factory lookup tables.
  * **Bad:** Class `my_env extends uvm_env` using `` `uvm_object_utils(my_env) ``.
  * **Good:** `` `uvm_component_utils(my_env) ``
  * **uvmcheck Status:** Planned

* **`UVM-FAC-003`**
  * **Title:** Missing UVM Factory Registration Macro
  * **Severity:** WARNING
  * **Description:** Concrete classes derived from `uvm_object` or `uvm_component` should be registered with the factory using the appropriate utility macro unless explicitly abstract.
  * **uvmcheck Status:** Planned

* **`UVM-FAC-004`**
  * **Title:** Parameterized Class Factory Registration
  * **Severity:** ERROR
  * **Description:** Parameterized UVM components and objects must use `` `uvm_component_param_utils(T) `` or `` `uvm_object_param_utils(T) `` instead of standard utils macros.
  * **uvmcheck Status:** Planned

---

### 1.2 Phasing and Lifecycle Execution
* **`UVM-PHS-001`**
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

* **`UVM-PHS-002`**
  * **Title:** Time-Consuming Statement in Function Phase
  * **Severity:** ERROR
  * **Description:** Function-based phases (`build_phase`, `connect_phase`, `extract_phase`, `check_phase`, `report_phase`, `final_phase`) must execute in zero simulation time. Delay `#`, clock `@`, and `wait` statements are prohibited.
  * **uvmcheck Status:** **Implemented**

* **`UVM-PHS-003`**
  * **Title:** Phase Method Signature Mismatch
  * **Severity:** ERROR
  * **Description:** Standard UVM phases must match the expected prototype (`function void <phase>(uvm_phase phase)` or `task run_phase(uvm_phase phase)`). Declaring `build_phase` as a task or without `uvm_phase phase` causes UVM to silently ignore the phase.
  * **uvmcheck Status:** Planned

---

### 1.3 Objection Management
* **`UVM-OBJ-001`**
  * **Title:** Unpaired `raise_objection` / `drop_objection`
  * **Severity:** ERROR
  * **Description:** Every `phase.raise_objection()` must have a corresponding, guaranteed `phase.drop_objection()` along all execution paths. Missing drops cause simulation to hang.
  * **uvmcheck Status:** Planned (Requires Control-Flow Graph analysis)

* **`UVM-OBJ-002`**
  * **Title:** Raising Objection Inside a Loop
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Raising and dropping objections inside high-frequency transaction loops incurs severe scheduler overhead. Objections should be raised once outside the transaction loop.
  * **uvmcheck Status:** Planned

* **`UVM-OBJ-003`**
  * **Title:** Top Test Run Phase Lacks Objections
  * **Severity:** WARNING
  * **Description:** Top-level `uvm_test::run_phase` should raise an objection before starting background sequences to prevent simulation from terminating at time 0.
  * **uvmcheck Status:** Planned

---

### 1.4 Configuration Database (`uvm_config_db`)
* **`UVM-CFG-001`**
  * **Title:** Inconsistent `uvm_config_db` Type Parameter
  * **Severity:** WARNING
  * **Description:** The type parameter `T` in `uvm_config_db#(T)::set(...)` must match the type parameter in the corresponding `uvm_config_db#(T)::get(...)`. Mismatches cause silent lookup failures.
  * **uvmcheck Status:** Planned

* **`UVM-CFG-002`**
  * **Title:** Null Context in Component `config_db` Call
  * **Severity:** WARNING
  * **Description:** Inside a `uvm_component`, calling `uvm_config_db#(T)::get(null, ...)` breaks hierarchical scoping. Use `this` as the context handle.
  * **uvmcheck Status:** Planned

* **`UVM-CFG-003`**
  * **Title:** Unchecked Return Value on `config_db::get`
  * **Severity:** WARNING
  * **Description:** `uvm_config_db#(T)::get(...)` returns a boolean indicating whether the configuration was found. Critical items (like virtual interfaces) must check the return value and issue `uvm_fatal` if missing.
  * **uvmcheck Status:** Planned

---

### 1.5 TLM Connections and Sequences
* **`UVM-TLM-001`**
  * **Title:** Unconnected TLM Port in Connect Phase
  * **Severity:** ERROR
  * **Description:** Mandatory TLM ports (such as `uvm_blocking_put_port`) left unconnected after `end_of_elaboration_phase` result in runtime fatal errors when called.
  * **uvmcheck Status:** Planned

* **`UVM-SEQ-001`**
  * **Title:** Sequence Directly Accessing Physical Interface
  * **Severity:** ARCHITECTURE / WARNING
  * **Description:** Sequences should only create and randomize transactions; they should not contain hierarchical references to physical DUT interfaces (`vif.sig <= 1`). Signal driving belongs exclusively to the driver.
  * **uvmcheck Status:** Planned

* **`UVM-SEQ-002`**
  * **Title:** Unpaired `start_item` and `finish_item`
  * **Severity:** ERROR
  * **Description:** Calling `start_item(req)` without a subsequent `finish_item(req)` locks the sequencer queue and starves drivers.
  * **uvmcheck Status:** Planned

---

## Category 2: SystemVerilog OOP & Semantics (`OOP.*` / `SEMANTIC.*`)

* **`SV-OOP-001`**
  * **Title:** Overriding Non-Virtual Method in Base Class
  * **Severity:** WARNING
  * **Description:** Overriding a method that was declared non-virtual in a base class breaks polymorphism. Calls through base-class handles will execute the base implementation rather than the derived one.
  * **uvmcheck Status:** **Implemented**

* **`SV-OOP-002`**
  * **Title:** Unchecked Return Value of `$cast(dest, src)`
  * **Severity:** WARNING
  * **Description:** When `$cast` is called as a function (or cast to `void'`), failing casts leave `dest` as `null` without stopping execution, triggering null-pointer dereference crashes downstream.
  * **Bad:** `void'($cast(tx, item));`
  * **Good:** `if (!$cast(tx, item)) \`uvm_fatal("CASTERR", "Failed cast");`
  * **uvmcheck Status:** Planned

* **`SV-OOP-003`**
  * **Title:** Missing `super.new()` in Class Constructor
  * **Severity:** WARNING
  * **Description:** Derived class constructors should explicitly call `super.new(...)` with appropriate arguments to guarantee base class fields and parent identifiers are properly initialized.
  * **uvmcheck Status:** Planned

* **`SV-OOP-004`**
  * **Title:** Illegal Direct Access to `local` or `protected` Members
  * **Severity:** ERROR
  * **Description:** Flags illegal attempts to bypass object encapsulation.
  * **uvmcheck Status:** Planned

---

## Category 3: Simulation Performance Optimization (`PERF.*`)

* **`SV-PRF-001`**
  * **Title:** Passing Large Array or Struct by Value
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Passing dynamic arrays, queues, associative arrays, or large structs into tasks/functions by value triggers expensive deep-memory copies on every invocation. Use `const ref` or `ref` instead.
  * **Bad:** `function void process_packet(byte payload[]);`
  * **Good:** `function void process_packet(const ref byte payload[]);`
  * **uvmcheck Status:** Planned

* **`SV-PRF-002`**
  * **Title:** Heavy String Formatting in High-Speed Loops
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Calling `$sformatf` inside tight simulation loops when messages are filtered by verbosity anyway incurs high memory allocation penalties.
  * **uvmcheck Status:** Planned

* **`SV-PRF-003`**
  * **Title:** Inefficient Large `inside` Range in Constraint
  * **Severity:** PERFORMANCE / WARNING
  * **Description:** Constraints with unbounded sets or deeply nested implications stress BDD/SAT solver engines.
  * **uvmcheck Status:** Planned

---

## Category 4: Language Pitfalls & Thread Concurrency (`PITFALL.*`)

* **`SV-THR-001`**
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

* **`SV-THR-002`**
  * **Title:** Background Thread Leak in Task
  * **Severity:** WARNING
  * **Description:** Tasks that launch `fork..join_any` or `fork..join_none` without `disable fork` leave orphan background processes running after the enclosing task exits.
  * **uvmcheck Status:** Planned

---

## Category 5: Dead Code & Redundancy (`DEADCODE.*`)

* **`SV-DED-001`**
  * **Title:** Unreferenced Local Variable
  * **Severity:** INFO / WARNING
  * **Description:** Variables declared inside tasks, functions, or class scopes that are never read or written.
  * **uvmcheck Status:** Planned

* **`SV-DED-002`**
  * **Title:** Uncalled Private Function or Task
  * **Severity:** INFO
  * **Description:** Local methods with `local` visibility that are never referenced within the class scope.
  * **uvmcheck Status:** Planned

---

## Category 6: Functional Coverage & Assertions (`COVERAGE.*` / `SVA.*`)

* **`SV-COV-001`**
  * **Title:** Coverpoint Lacks Explicit Bins
  * **Severity:** WARNING
  * **Description:** A coverpoint declared on a wide integer (e.g. 32-bit int) without explicit bins defaults to 64 auto-bins, resulting in misleading coverage statistics.
  * **uvmcheck Status:** Planned

* **`SV-SVA-001`**
  * **Title:** Concurrent Assertion in Dynamic Scope
  * **Severity:** ERROR
  * **Description:** Concurrent assertions (`assert property`) require static clocking domains and cannot be placed inside dynamic class objects.
  * **uvmcheck Status:** Planned

---

## Category 7: Naming Conventions & Style (`STYLE.*`)

* **`SV-STY-001`**
  * **Title:** Class Property Missing Prefix
  * **Severity:** STYLE
  * **Description:** Enforces that member properties of classes begin with `m_` (e.g. `m_cfg`, `m_pkt_count`) to distinguish local state from parameters and arguments.
  * **uvmcheck Status:** Planned

* **`SV-STY-002`**
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

---

## Category 9: Methodology Guide Rules (`UVM-RUS`, `UVM-ROL`, `UVM-CPL`, `UVM-PAT`, `RAL-ADP`, `UVM-REP-007/008`, `SV-PKG`, `SV-CNS`, `SV-CPL`, `UVM-CFG-007` to `009`, `SV-DUP-002` to `005`, `SV-EVO`)

Rules written for the [UVM Methodology Guide](../methodology/README.md). Each enforces one guideline; `--guideline <id>` runs a chapter or guideline, and `--list-guidelines` prints the full mapping, including the existing rules tagged with guidelines.

### 9.1 Reusable verification components (MG-4)

* **`UVM-RUS-001`** (AgentIgnoresIsActive)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.1](../methodology/README.md)
  * **Description:** Flags agents that create their driver or sequencer without checking is_active or get_is_active(). Such an agent cannot be reused as a passive monitor in a larger env (MG-4.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-RUS-002`** (AgentWithoutConfigObject)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.2](../methodology/README.md)
  * **Description:** Flags agents and envs that read three or more separate scalar settings from uvm_config_db instead of one config object (Fowler 'data clumps', MG-4.2).
  * **uvmcheck Status:** **Implemented**

* **`UVM-RUS-003`** (HardcodedTopology)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.2](../methodology/README.md)
  * **Description:** Flags envs that create components in a loop with a literal count instead of a count from their config object (open/closed principle, MG-4.2).
  * **uvmcheck Status:** **Implemented**

* **`UVM-RUS-004`** (AbsoluteConfigDbPath)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.3](../methodology/README.md)
  * **Description:** Flags uvm_config_db calls in classes whose path starts at uvm_test_top. Below the test the path should be relative to 'this'; with a non-null context an absolute path is appended to the context and never matches (MG-4.3).
  * **uvmcheck Status:** **Implemented**

* **`UVM-RUS-005`** (OverrideByNameString)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.3](../methodology/README.md)
  * **Description:** Flags factory overrides and creates that name types with string literals (set_type_override_by_name, set_inst_override, create_object ...). A typo or rename is only found at run time; T::type_id::set_type_override() is checked by the compiler (connascence of name, MG-4.3).
  * **uvmcheck Status:** **Implemented**

* **`UVM-RUS-006`** (IncompleteItemContract)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-4.4](../methodology/README.md)
  * **Description:** Flags concrete sequence items with fields that implement neither field macros nor do_copy(), do_compare() and convert2string()/do_print(). Scoreboards, clone() and debug prints then silently see default behaviour (MG-4.4).
  * **uvmcheck Status:** **Implemented**

### 9.2 Component roles (MG-3)

* **`UVM-ROL-001`** (DriverGeneratesStimulus)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-3.1](../methodology/README.md)
  * **Description:** Flags drivers that randomize sequence items or start sequences. Stimulus belongs in sequences, where tests can layer constraints and the sequencer can arbitrate; a driver only converts items to pin activity (single responsibility, MG-3.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ROL-002`** (CoverageOutsideObservers)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-3.1](../methodology/README.md)
  * **Description:** Flags covergroups declared in drivers and sequences. They sample what was intended, not what reached the DUT, and disappear when the agent is passive; collect coverage in a monitor or subscriber (MG-3.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ROL-003`** (DutCheckingInAgent)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-3.2](../methodology/README.md)
  * **Description:** Flags agents that hold a scoreboard, predictor or reference model. Agents carry protocol knowledge only; DUT-specific checking belongs in the env so the agent can be reused with other DUTs (MG-3.2).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ROL-004`** (TestBuildsLeafComponents)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-3.3](../methodology/README.md)
  * **Description:** Flags tests that create drivers, monitors, sequencers, agents, scoreboards or subscribers directly. Tests configure and select; the env builds, so every test shares one structure (facade, MG-3.3).
  * **uvmcheck Status:** **Implemented**

* **`UVM-ROL-005`** (MonitorDependsOnActivePath)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-3.1](../methodology/README.md)
  * **Description:** Flags monitors that hold a handle to a driver, sequencer or sequence item port. A monitor must work on its own, observing only the interface, or it breaks in passive agents (MG-3.1).
  * **uvmcheck Status:** **Implemented**

### 9.3 Coupling (MG-2)

* **`UVM-CPL-002`** (HandleToEnvOrTest)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.1](../methodology/README.md)
  * **Description:** Flags sequences, items, agents, config objects and helpers that store a handle to a user env or test class. They then depend on everything the env contains and cannot be used under another env (stamp coupling, dependency inversion, MG-2.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-CPL-003`** (ForeignComponentStateWrite)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.1](../methodology/README.md)
  * **Description:** Flags assignments to properties of another component through a handle, except a parent setting up its own subtree in new() and the build, connect and end_of_elaboration phases (content coupling, MG-2.1). UVM-ARC-002 covers long read chains.
  * **uvmcheck Status:** **Implemented**

* **`UVM-CPL-004`** (HierarchicalReferenceInClass)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.2](../methodology/README.md)
  * **Description:** Flags class code that names a module hierarchy (tb_top.dut.sig, $root.tb.x). Classes must reach the DUT only through virtual interfaces so they stay reusable (information hiding, MG-2.2).
  * **uvmcheck Status:** **Implemented**

### 9.4 Pattern conformance and failure handling (MG-5, MG-8)

* **`UVM-PAT-001`** (CallbackNotRegistered)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-5.1](../methodology/README.md)
  * **Description:** Flags `uvm_do_callbacks(T, CB, ...) in classes where no `uvm_register_cb(T, CB) registers that pair for T or a base of T. UVM then refuses to add callbacks of that type at run time, so the hook never fires (MG-5.1).
  * **uvmcheck Status:** **Implemented**

* **`RAL-ADP-001`** (IncompleteRegAdapter)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-5.1](../methodology/README.md)
  * **Description:** Flags uvm_reg_adapter subclasses whose bus2reg() never sets rw.status or whose reg2bus() never reads rw.kind. The register layer then reports every access as passed, or turns reads into writes (adapter pattern, MG-5.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-REP-007`** (CheckFailureAsWarning)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-8.1](../methodology/README.md)
  * **Description:** Flags a failed compare() or an expected-versus-actual mismatch reported only with `uvm_warning or `uvm_info. The test still passes; check failures must be errors so they are counted (MG-8.1).
  * **uvmcheck Status:** **Implemented**

* **`UVM-REP-008`** (FatalInCheckerPath)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-8.1](../methodology/README.md)
  * **Description:** Flags `uvm_fatal on a data mismatch in scoreboards and subscribers. The run stops at the first mismatch and hides how many others there are; use `uvm_error and let max_quit_count stop the run (MG-8.1).
  * **uvmcheck Status:** **Implemented**

### 9.5 Layering and packages (MG-1)

* **`SV-PKG-001`** (FileClassNameMismatch)
  * **Severity:** WARNING · **Category:** Architecture · **Guideline:** [MG-1.2](../methodology/README.md)
  * **Description:** Flags `include'd class files that define more than one top-level class, or whose one class is not named after the file (my_drv.svh holding my_driver). With one class per file, named after it, a class can be found from its name and reused on its own (MG-1.2).
  * **uvmcheck Status:** **Implemented**

* **`SV-PKG-002`** (WildcardImportOutsidePackage)
  * **Severity:** WARNING · **Category:** Architecture · **Guideline:** [MG-1.3](../methodology/README.md)
  * **Description:** Flags 'import pkg::*' at file ($unit) scope, and inside files that are `include'd into a package. Both leak names into code that never asked for them and hide where a dependency comes from (information hiding, MG-1.3).
  * **uvmcheck Status:** **Implemented**

* **`SV-PKG-003`** (UnusedPackageImport)
  * **Severity:** WARNING · **Category:** Architecture · **Guideline:** [MG-1.3](../methodology/README.md)
  * **Description:** Flags package imports that supply no name used in their scope. An unused import is a false dependency: the package is compiled first, changes to it look relevant, and names it exports can collide with local ones (common reuse principle, MG-1.3).
  * **uvmcheck Status:** **Implemented**

* **`SV-PKG-004`** (PackageLayerInversion)
  * **Severity:** WARNING · **Category:** Architecture · **Guideline:** [MG-1.1](../methodology/README.md)
  * **Description:** Flags a package that imports a package from a higher testbench layer, such as an agent package importing an env or test package. The lower package can then not be compiled or reused without the higher one (acyclic and stable dependencies, MG-1.1). UVM-ARC-001 checks the same order between classes.
  * **uvmcheck Status:** **Implemented**

* **`SV-PKG-005`** (UnstablePackageDependedOn)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-1.4](../methodology/README.md)
  * **Description:** Computes instability I = Ce / (Ca + Ce) for each user package from its imports and flags a package that depends on a less stable one. Many packages rely on a stable package, so its own dependencies should be at least as stable (Martin's stable dependencies principle, MG-1.4). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

### 9.6 config_db coupling (MG-2)

* **`UVM-CFG-007`** (ConfigDbAsRuntimeChannel)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.3](../methodology/README.md)
  * **Description:** Flags uvm_config_db get, set, exists and wait_modified calls in the run-time phases of components and inside loops of any task. config_db is build-time wiring; used at run time it becomes a slow global mailbox (common coupling, MG-2.3).
  * **uvmcheck Status:** **Implemented**

* **`UVM-CFG-008`** (ScatteredConfigKeyLiteral)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.4](../methodology/README.md)
  * **Description:** Flags a uvm_config_db field name typed as a string literal in two or more files. The setter and getter only meet if both spell it the same; a shared constant makes the compiler check that (connascence of name, MG-2.4). Reported once per name.
  * **uvmcheck Status:** **Implemented**

* **`UVM-CFG-009`** (SharedConfigMutatedAtRuntime)
  * **Severity:** WARNING · **Category:** UVM · **Guideline:** [MG-2.3](../methodology/README.md)
  * **Description:** Flags writes to fields of a config object that the class got from uvm_config_db and did not create, made outside new() and the build phases. The object is shared by everyone who read it, so the write is hidden global state (common coupling, MG-2.3).
  * **uvmcheck Status:** **Implemented**

* **`SV-CPL-001`** (StringModeParameter)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-2.4](../methodology/README.md)
  * **Description:** Flags routines whose string argument is compared against two or more literals to choose what to do, as in do_op("read"). Caller and callee must agree on spellings the compiler never checks (control coupling, connascence of meaning, MG-2.4). Use an enum argument or separate routines. Overrides are left alone.
  * **uvmcheck Status:** **Implemented**

### 9.7 Construct usage (MG-6)

* **`SV-CNS-001`** (DefineForConstant)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-6.1](../methodology/README.md)
  * **Description:** Flags `define macros whose body is only a number or a string, used as constants. A package parameter or localparam has a type and a scope; a macro is global to the rest of the compilation and can be redefined silently (MG-6.1).
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-002`** (MacroAsRoutine)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-6.1](../methodology/README.md)
  * **Description:** Flags multi-statement code macros that use nothing only a macro can do: no token pasting, stringification, nested macros, `__FILE__, type arguments, declarations or return/break. A function or task would be type-checked and debuggable (MG-6.1). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-003`** (IfdefInReusableClass)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-6.1](../methodology/README.md)
  * **Description:** Flags `ifdef, `ifndef and `elsif inside class bodies, apart from simulator and UVM version shims. Compile-time switches multiply the variants of a class and cannot be chosen per instance; configuration can (MG-6.1).
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-004`** (StringAsEnum)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-6.2](../methodology/README.md)
  * **Description:** Flags a string variable compared against three or more literals in one case statement or if/else-if chain. The values form an enumeration the compiler cannot check; a typo in any of them fails silently (Fowler 'primitive obsession', MG-6.2). String arguments are SV-CPL-001's.
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-005`** (ManualBitFieldSlicing)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-6.2](../methodology/README.md)
  * **Description:** Flags a vector that is cut into three or more different fixed part-selects (hdr[7:0], hdr[15:8], hdr[31:16]). The field layout lives in scattered numbers; a packed struct names it once (MG-6.2). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-006`** (ImplicitEnumBaseType)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-6.3](../methodology/README.md)
  * **Description:** Flags enums in packages, classes and interfaces declared without a base type. They silently become 32-bit int, which wastes bits in packed items and no longer matches the width of the bus field they model (MG-6.3). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-007`** (LegacyIntegerType)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-6.3](../methodology/README.md)
  * **Description:** Flags the Verilog-1995 type 'integer' in class code. It is a 4-state 32-bit signed value; int (2-state) or an explicitly sized logic vector says what is meant (MG-6.3). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-CNS-008`** (FunctionOutputArguments)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-6.3](../methodology/README.md)
  * **Description:** Flags void class functions that hand their results back through output arguments. A return value, or a struct for several values, keeps the call an expression and makes the data flow visible (command-query separation, MG-6.3). Overrides are left alone, since their signature is fixed. Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

### 9.8 Duplication (MG-7)

* **`SV-DUP-002`** (CloneFragments)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-7.1](../methodology/README.md)
  * **Description:** Finds runs of three or more statements (at least 50 tokens) that appear in more than one place, also when variables and literals were renamed consistently. Macro calls count as written, not as their expansion, and called names must match. Each pair of routines is reported once; whole identical method bodies are SV-DUP-001's, and files whose header says they are generated are left out (DRY, Type-1/2 clones, MG-7.1).
  * **uvmcheck Status:** **Implemented**

* **`SV-DUP-003`** (DuplicatedConstraint)
  * **Severity:** WARNING · **Category:** Maintainability · **Guideline:** [MG-7.1](../methodology/README.md)
  * **Description:** Flags a constraint body repeated in two classes that do not inherit from each other. The rule then lives in two places and drifts; declare it once in a shared base class or a policy object (Fowler 'pull up', MG-7.1). SV-RND-011 covers repeats inside one class.
  * **uvmcheck Status:** **Implemented**

* **`SV-DUP-004`** (RepeatedStringLiteral)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-7.1](../methodology/README.md)
  * **Description:** Flags a string literal, such as a report ID or a file name, written four or more times in the code. Config keys (UVM-CFG-008), instance names and class names are left out. One named constant keeps the copies in step (DRY, MG-7.1). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-DUP-005`** (CopiedAgentPackage)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-7.2](../methodology/README.md)
  * **Description:** Flags two packages whose classes have the same roles, fields and method bodies apart from renamed identifiers, for at least three classes and 80% of the classes in the larger package (classes with almost no code are not counted). They are one agent copied and renamed; a parameterized agent, or a shared base with small subclasses, keeps one copy to fix (DRY, MG-7.2). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

### 9.9 Change history (MG-9)

These read git and run only with `--history`.

* **`SV-EVO-001`** (Hotspot)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-9.2](../methodology/README.md)
  * **Description:** With --history: reports the files that are both complex (summed cyclomatic complexity of their routines) and often changed in git, the top five by complexity times commits. Such hotspots are where defects collect and where refactoring pays first (Tornhill, MG-9.2). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**

* **`SV-EVO-002`** (ChangeCoupling)
  * **Severity:** INFO · **Category:** Advisory · **Guideline:** [MG-9.2](../methodology/README.md)
  * **Description:** With --history: reports two files in different packages that change together in at least three commits and in at least half of the commits touching either. The packages have a hidden dependency that their imports do not show; move the shared knowledge to one place (temporal coupling, MG-9.2). Advisory: off by default.
  * **uvmcheck Status:** **Implemented**
