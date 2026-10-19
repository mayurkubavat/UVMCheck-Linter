// Test Case: SV-STY-001 (ClassMemberPrefix)
// Extracted from sample_tb.sv (Test Case 15)

`include "uvm_mock.svh"

// --- TEST CASE 15: SV-STY-001 (Class Member Prefix) ---
class BadPrefixComponent extends uvm_component;
    `uvm_component_utils(BadPrefixComponent)
    // VIOLATION SV-STY-001: Member property missing 'm_' prefix!
    int packet_counter;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass
