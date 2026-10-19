// Test Case: SV-DUP-001 (DuplicateMethodBody)
// Extracted from sample_tb.sv (Test Case 13)

`include "uvm_mock.svh"

// --- TEST CASE 13: SV-DUP-001 (Duplicate Method Body / Code Duplication) ---
class CrcCalculatorAlpha extends uvm_object;
    `uvm_object_utils(CrcCalculatorAlpha)
    function new(string name = "CrcCalculatorAlpha");
        super.new(name);
    endfunction

    function int compute_crc(int data, int seed);
        int result = seed ^ data;
        result = (result << 1) | (result >> 31);
        result = result ^ 32'h04C11DB7;
        return result;
    endfunction
endclass

class CrcCalculatorBeta extends uvm_object;
    `uvm_object_utils(CrcCalculatorBeta)
    function new(string name = "CrcCalculatorBeta");
        super.new(name);
    endfunction

    // VIOLATION SV-DUP-001: Identical copy-pasted method body as compute_crc in CrcCalculatorAlpha!
    function int compute_crc(int data, int seed);
        int result = seed ^ data;
        result = (result << 1) | (result >> 31);
        result = result ^ 32'h04C11DB7;
        return result;
    endfunction
endclass
