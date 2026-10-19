// Test Case: UVM-DAG-001 (UvmDanglingConfigString)
// Extracted from sample_tb.sv (Test Case 83)

`include "uvm_mock.svh"

// --- TEST CASE 83: UVM-DAG-001 (Dangling UVM config_db Path String) ---
class DanglingConfigTest extends uvm_component;
    `uvm_component_utils(DanglingConfigTest)

    function new(string name = "DanglingConfigTest", uvm_component parent = null);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION UVM-DAG-001: String path 'uvm_test_top.m_env.m_typo_agent' matches 0 components in the static DAG!
        uvm_config_db#(int)::set(this, "uvm_test_top.m_env.m_typo_agent", "is_active", 0);
    endfunction
endclass
