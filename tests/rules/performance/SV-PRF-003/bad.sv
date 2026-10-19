// Test Case: SV-PRF-003 (DynArrayIncrementalResizeInLoop)
// Extracted from sample_tb.sv (Test Case 36)

`include "uvm_mock.svh"

// --- TEST CASE 36: SV-PRF-003 (Incremental Dynamic Array Resize in Loop) ---
class DynArrayResizeTester extends uvm_object;
    `uvm_object_utils(DynArrayResizeTester)
    function new(string name = "DynArrayResizeTester");
        super.new(name);
    endfunction

    function void build_buffer();
        int buffer[];
        for (int i = 0; i < 8; i++) begin
            // VIOLATION SV-PRF-003: Incremental reallocation new[+1](buffer) inside loop causes O(N^2) memory copying!
            buffer = new[buffer.size() + 1](buffer);
            buffer[i] = i;
        end
    endfunction
endclass
