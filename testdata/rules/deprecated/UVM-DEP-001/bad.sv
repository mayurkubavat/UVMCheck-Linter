// Test Case: UVM-DEP-001 (UvmDeprecatedConfigCalls)
// Extracted from sample_tb.sv (Test Case 54)

`include "uvm_mock.svh"

// --- TEST CASE 54: UVM-DEP-001 (UVM Deprecated Config Calls) ---
class DeprecatedConfigComponent extends uvm_component;
    `uvm_component_utils(DeprecatedConfigComponent)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-DEP-001: set_config_int is deprecated in IEEE 1800.2 Annex H!
        set_config_int("*.agent", "is_active", 1);
    endfunction
endclass
