// Test Case: SV-RND-002 (GlobalRngInClass)
// Extracted from sample_tb.sv (Test Case 21)

`include "uvm_mock.svh"

// --- TEST CASE 21: SV-RND-002 (Global RNG System Call Inside Class) ---
class GlobalRngComponent extends uvm_component;
    `uvm_component_utils(GlobalRngComponent)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function int get_delay();
        // VIOLATION SV-RND-002: Calling $urandom inside a class breaks seed repeatability!
        return $urandom_range(10, 50);
    endfunction
endclass
