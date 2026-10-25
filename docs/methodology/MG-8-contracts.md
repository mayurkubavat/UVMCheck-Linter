# MG-8 Contracts and failure handling

Grounded in Meyer's design by contract and the fail-fast principle.

A check that cannot fail the test is not a check. Failures go through the UVM report server at error severity, so they are counted, can be waived deliberately and appear in the summary.

## MG-8.1 (Shall)

**Every check that can fail reports through UVM at error severity, so failures are counted and never hidden.**

A mismatch reported as a warning lets the test pass; `uvm_fatal on the first mismatch hides how many there were. Unchecked randomize(), $cast() and config_db get() results fail silently.

Instead of:

```systemverilog
if (!act.compare(exp)) `uvm_warning("SB", "mismatch")
```

Write:

```systemverilog
if (!act.compare(exp)) `uvm_error("SB", "mismatch")
```

Enforced by: `UVM-REP-007`, `UVM-REP-008`, `SV-ERR-001`, `SV-ERR-002`, `SV-ERR-003`, `SV-OOP-002`, `SV-RND-001`, `UVM-CFG-004`, `UVM-TST-001`, `UVM-REP-006`, `UVM-SCB-001`, `UVM-SCB-002`, `UVM-SEQ-006`, `UVM-TLM-002`, `SV-IPC-001`, `SV-SYS-001`, `SVA-ACT-001`, `UVM-LOG-002`

## MG-8.2 (Should)

**Reusable interfaces carry their own protocol assertions.**

An interface that carries its own assertions checks the protocol in every testbench that uses it, including ones written later.

Enforced by: review only (no rule yet).
