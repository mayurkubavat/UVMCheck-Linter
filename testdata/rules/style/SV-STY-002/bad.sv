// Test Case: SV-STY-002 (MacroTrailingSemicolon)
// Extracted from sample_tb.sv (Test Case 5)

`include "uvm_mock.svh"

// --- TEST CASE 5: SV-STY-002 (Trailing semicolon after macro call) ---
class StyleViolationComponent extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION SV-STY-002: Trailing semicolon after macro invocation in declarative scope
    `uvm_component_utils(StyleViolationComponent);
endclass
