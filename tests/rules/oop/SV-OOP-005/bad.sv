// Test Case: SV-OOP-005 (VirtualCallInConstructor)
// Extracted from sample_tb.sv (Test Case 19)

`include "uvm_mock.svh"

// --- TEST CASE 19: SV-OOP-005 (Virtual Call in Constructor) ---
class VirtualCallCtorClass extends uvm_object;
    `uvm_object_utils(VirtualCallCtorClass)
    function new(string name = "VirtualCallCtorClass");
        super.new(name);
        // VIOLATION SV-OOP-005: Invoking virtual method setup_defaults() inside constructor!
        setup_defaults();
    endfunction

    virtual function void setup_defaults();
    endfunction
endclass
