// Test Case: SV-DED-002 (UnreachableStatementsPostReturn)
// Extracted from sample_tb.sv (Test Case 33)

`include "uvm_mock.svh"

// --- TEST CASE 33: SV-DED-002 (Unreachable Statements Post Return) ---
class UnreachableCodeTester extends uvm_object;
    `uvm_object_utils(UnreachableCodeTester)
    function new(string name = "UnreachableCodeTester");
        super.new(name);
    endfunction

    function int compute_val(int in_val);
        int out_val = in_val * 2;
        return out_val;
        // VIOLATION SV-DED-002: Statement appears after unconditional return!
        out_val = out_val + 10;
    endfunction
endclass
