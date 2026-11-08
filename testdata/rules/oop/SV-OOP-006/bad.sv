// Test Case: SV-OOP-006 (ClassShallowCopyAliasing)
// Extracted from sample_tb.sv (Test Case 24)

`include "uvm_mock.svh"

// --- TEST CASE 24: SV-OOP-006 (Class Shallow Copy Aliasing) ---
class PacketHeader;
    int addr;
endclass

class NetworkPacket extends uvm_sequence_item;
    `uvm_object_utils(NetworkPacket)
    int seq_id;
    PacketHeader hdr;

    function new(string name = "NetworkPacket");
        super.new(name);
        hdr = new();
    endfunction

    function void duplicate_unsafe();
        NetworkPacket p2;
        // VIOLATION SV-OOP-006: Shallow copy 'new this' leaves 'hdr' handle aliased between instances!
        p2 = new this;
    endfunction
endclass
