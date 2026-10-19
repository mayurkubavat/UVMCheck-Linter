// Test Case: UVM-OBS-001 (ObserverStateMutationInWrite)
// Extracted from sample_tb.sv (Test Case 31)

`include "uvm_mock.svh"

class NetworkPacket extends uvm_sequence_item;
    int seq_id;
endclass

// --- TEST CASE 31: UVM-OBS-001 (Observer State Mutation In write()) ---
class MutatingScoreboard extends uvm_component;
    `uvm_component_utils(MutatingScoreboard)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void write(NetworkPacket t);
        // VIOLATION UVM-OBS-001: Mutating broadcasted transaction object in-place corrupts other subscribers!
        t.seq_id = 100;
    endfunction
endclass
