// Test Case: SV-RND-003 (RandomizeMissingSuper)
// Extracted from sample_tb.sv (Test Case 25)

`include "uvm_mock.svh"

// --- TEST CASE 25: SV-RND-003 (Randomize Missing Super Call) ---
class BaseTransaction extends uvm_sequence_item;
    `uvm_object_utils(BaseTransaction)
    function new(string name = "BaseTransaction");
        super.new(name);
    endfunction

    function void post_randomize();
        // Base class CRC / parity calculation
    endfunction
endclass

class DerivedTransaction extends BaseTransaction;
    `uvm_object_utils(DerivedTransaction)
    function new(string name = "DerivedTransaction");
        super.new(name);
    endfunction

    function void post_randomize();
        // VIOLATION SV-RND-003: Overriding post_randomize() without calling super.post_randomize()!
        int vlan = 1;
    endfunction
endclass
