// Test Case: SV-LANG-001 (EqualityWithXz)
// Extracted from sample_tb.sv (Test Case 92)

`include "uvm_mock.svh"

// --- TEST CASE 92: SV-LANG-001 (EqualityWithXz) ---
class EqualityWithXzBench;
    function void check_packet(logic [7:0] data);
        // VIOLATION SV-LANG-001: 4-state equality comparison with 8'hxx evaluates to 'x (false)!
        if (data == 8'hxx) begin
            $display("Packet is corrupted with X");
        end
        if (data != 1'bx) begin
            $display("Valid data");
        end
    endfunction
endclass
