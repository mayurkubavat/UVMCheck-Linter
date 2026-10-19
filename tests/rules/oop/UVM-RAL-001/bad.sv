// Test Case: UVM-RAL-001 (DirectRalMirrorTampering)
// Extracted from sample_tb.sv (Test Case 32)

`include "uvm_mock.svh"

// --- TEST CASE 32: UVM-RAL-001 (Direct RAL Mirror Tampering) ---
class dummy_reg extends uvm_object;
    int m_value;
    int m_mirrored;
    `uvm_object_utils(dummy_reg)
    function new(string name = "dummy_reg");
        super.new(name);
    endfunction
endclass

class RalTamperComponent extends uvm_component;
    `uvm_component_utils(RalTamperComponent)
    dummy_reg m_ctrl_reg;

    function new(string name, uvm_component parent);
        super.new(name, parent);
        m_ctrl_reg = new("m_ctrl_reg");
    endfunction

    function void tamper_mirror();
        // VIOLATION UVM-RAL-001: Direct assignment to internal RAL property m_value bypasses predict()!
        m_ctrl_reg.m_value = 32'hDEAD_BEEF;
    endfunction
endclass
