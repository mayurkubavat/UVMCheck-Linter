// Test Case: SV-OOP-004 (VariableShadowing)
// Extracted from sample_tb.sv (Test Case 12)

`include "uvm_mock.svh"

// --- TEST CASE 12: SV-OOP-004 (Variable Shadowing) ---
class ShadowBase extends uvm_object;
    `uvm_object_utils(ShadowBase)
    int m_count;
    function new(string name = "ShadowBase");
        super.new(name);
    endfunction
endclass

class ShadowDerived extends ShadowBase;
    `uvm_object_utils(ShadowDerived)
    // VIOLATION SV-OOP-004: Property 'm_count' shadows base class property!
    int m_count;

    function new(string name = "ShadowDerived");
        super.new(name);
    endfunction

    // VIOLATION SV-OOP-004: Method argument 'm_count' shadows class property!
    function void update_val(int m_count);
        this.m_count = m_count;
    endfunction
endclass
