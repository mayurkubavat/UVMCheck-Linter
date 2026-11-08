// Test Case: UVM-BAN-002 (UvmBannedUvmDoMacros)
// Extracted from sample_tb.sv (Test Case 59)

`include "uvm_mock.svh"

// --- TEST CASE 59: UVM-BAN-002 (UVM Banned `uvm_do Macros) ---
class BannedUvmDoSeq extends uvm_sequence;
    `uvm_object_utils(BannedUvmDoSeq)
    function new(string name = "BannedUvmDoSeq");
        super.new(name);
    endfunction

    task body();
        BannedFieldMacroItem item;
        // VIOLATION UVM-BAN-002: `uvm_do macro obscures driver handshake!
        `uvm_do(item)
    endtask
endclass
