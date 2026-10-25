# MG-7 Duplication

Grounded in DRY, the rule of three and the clone taxonomy (exact, renamed and near-miss clones), with Fowler's Pull Up Method as the usual fix.

Copied code drifts: a fix lands in one copy and not the others. The third copy is the signal to extract a base class, a helper or a parameterized class.

## MG-7.1 (Should)

**Copy-paste is refactored into a shared base, helper or parameterized class by the third copy.**

Copied code drifts as fixes land in one copy only.

Four rules look for copies at different sizes. SV-DUP-001 finds a whole method body repeated in another class. SV-DUP-002 finds a run of three or more statements (50 tokens by default) repeated anywhere, also when variables and literals were renamed consistently; a macro call counts as the call written, not as the code it expands to, and called routine names must match, so `raise_objection` and `drop_objection` are not taken for copies. SV-DUP-003 finds a constraint pasted into two classes that do not inherit from each other, and SV-RND-011 a constraint repeated inside one class. SV-DUP-004 (advisory) finds a string literal, such as a report ID, written four or more times. Files whose header says a tool generated them ("auto-generated", "do not edit") are left out, since their copies come from one template.

Instead of:

```systemverilog
function void drive_write(bus_item req);
  req.parity = ^req.data;
  last_addr = req.addr & 32'hFFFF_FFFC;
  sent++;
  `uvm_info("DRV", $sformatf("write %h", req.addr), UVM_HIGH)
endfunction

function void drive_read(bus_item rd);
  rd.parity = ^rd.data;
  last_addr = rd.addr & 32'hFFFF_FFFC;
  sent++;
  `uvm_info("DRV", $sformatf("read %h", rd.addr), UVM_HIGH)
endfunction
```

Write:

```systemverilog
function void finish(bus_item t, string kind);
  t.parity = ^t.data;
  last_addr = t.addr & 32'hFFFF_FFFC;
  sent++;
  `uvm_info(ID, $sformatf("%s %h", kind, t.addr), UVM_HIGH)
endfunction
```

The thresholds are options in the project config: `min_tokens` and `min_statements` for SV-DUP-002, `min_tokens` for SV-DUP-003 and `min_count` for SV-DUP-004.

Enforced by: `SV-DUP-001`, `SV-DUP-002`, `SV-DUP-003`, `SV-DUP-004`, `SV-RND-011`

## MG-7.2 (May)

**Near-identical agents become one parameterized agent.**

Two agents that differ only in names and widths are one parameterized agent waiting to happen.

SV-DUP-005 (advisory) compares packages class by class. A class's shape is its UVM role, its number of fields and its method bodies with identifiers renamed. When two packages share at least three shapes, covering 80% of the classes in the larger one, they are reported as one agent copied and renamed. Classes with almost no code are not counted, since small classes look alike by chance. The fix is a parameterized agent, or a shared base agent with small subclasses for what really differs.

Enforced by: `SV-DUP-005`
