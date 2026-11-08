// Compliant Test Case: SV-RND-023 (UnallocatedRandObjectArray)
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
    localparam int MAX_BEATS = 8;
    rand pkt_beat beats[MAX_BEATS];
    rand int unsigned len;
    constraint c_len { len inside {[1:MAX_BEATS]}; }
    function new(string name = "burst_item");
        super.new(name);
        foreach (beats[i]) beats[i] = pkt_beat::type_id::create($sformatf("beat%0d", i));
    endfunction : new
endclass : burst_item
