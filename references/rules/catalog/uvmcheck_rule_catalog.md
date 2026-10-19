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
