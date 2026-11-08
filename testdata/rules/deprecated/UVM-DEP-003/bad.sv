// Test Case: UVM-DEP-003 (UvmDeprecatedPrinterKnobs)
// Extracted from sample_tb.sv (Test Case 56)

`include "uvm_mock.svh"

// --- TEST CASE 56: UVM-DEP-003 (UVM Deprecated Printer Knobs) ---
module deprecated_knobs_tester;
    initial begin
        uvm_table_printer p;
        p = new("p");
        // VIOLATION UVM-DEP-003: printer.knobs is deprecated!
        p.knobs.depth = 3;
    end
endmodule
