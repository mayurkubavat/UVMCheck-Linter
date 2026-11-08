// Violation Test Case: SV-RND-023 (UnallocatedRandObjectArray)
`include "uvm_mock.svh"

class pkt_beat extends uvm_object;
    `uvm_object_utils(pkt_beat)
    rand bit [7:0] data;
    function new(string name = "pkt_beat");
        super.new(name);
    endfunction : new
endclass : pkt_beat

class burst_item extends uvm_sequence_item;
    `uvm_object_utils(burst_item)
    rand pkt_beat beats[];                 // VIOLATION: elements are never created
    constraint c_len { beats.size() inside {[1:8]}; }
    function new(string name = "burst_item");
        super.new(name);
    endfunction : new
endclass : burst_item
