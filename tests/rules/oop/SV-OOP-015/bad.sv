// Test Case: SV-OOP-015 (VirtualMethodSignatureMismatch)
// Extracted from sample_tb.sv (Test Case 223)

`include "uvm_mock.svh"

// --- TEST CASE 223: SV-OOP-015 (VirtualMethodSignatureMismatch) ---
class PolyBaseBench;
    virtual function void configure_device(int mode);
    endfunction : configure_device
endclass : PolyBaseBench

class PolyDerivedMismatchBench extends PolyBaseBench;
    // VIOLATION SV-OOP-015: Derived method has 2 parameters, overriding base with 1 parameter!
    function void configure_device(int mode, bit enable);
    endfunction : configure_device
endclass : PolyDerivedMismatchBench
