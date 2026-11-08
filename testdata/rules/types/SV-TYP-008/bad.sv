// Test Case: SV-TYP-008 (RedundantCastToSameType)
// Extracted from sample_tb.sv (Test Case 197)

`include "uvm_mock.svh"

class BasePacket;
endclass

// --- TEST CASE 197: SV-TYP-008 (RedundantCastToSameType) ---
class RedundantCastBench;
    BasePacket p1, p2;
    function void test_redundant_cast();
        p2 = new();
        // VIOLATION SV-TYP-008: $cast between identical static types 'BasePacket'!
        void'($cast(p1, p2));
    endfunction : test_redundant_cast
endclass : RedundantCastBench

// The operand is declared with the very type the cast names: redundant for
// every parameter value.
module param_cast_same_type #(parameter type data_t = logic) (input data_t d_i, output data_t d_o);
    // VIOLATION SV-TYP-008: d_i is already a data_t.
    assign d_o = data_t'(d_i);
endmodule
