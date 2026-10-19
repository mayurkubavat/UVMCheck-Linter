// Test Case: UVM-CFG-001 (UvmConfigDbTypeMismatch)
// Extracted from sample_tb.sv (Test Case 14)

`include "uvm_mock.svh"

// --- TEST CASE 14: UVM-CFG-001 (UVM Config DB Type Mismatch) ---
class ConfigMismatchEnv extends uvm_component;
    `uvm_component_utils(ConfigMismatchEnv)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        string mode_str;
        super.build_phase(phase);
        // Set with type 'int'
        uvm_config_db#(int)::set(this, "*", "retry_limit", 10);
        // VIOLATION UVM-CFG-001: Get with type 'string' for the same field 'retry_limit'!
        void'(uvm_config_db#(string)::get(this, "", "retry_limit", mode_str));
    endfunction
endclass
