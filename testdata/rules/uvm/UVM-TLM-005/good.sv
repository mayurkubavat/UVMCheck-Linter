// Test for UVM-TLM-005: Safe local copy before modifying transaction
`include "uvm_mock.svh"

class my_packet extends uvm_sequence_item;
    `uvm_object_utils(my_packet)
    bit [31:0] data;
    bit [15:0] addr;
    function new(string name = "my_packet");
        super.new(name);
    endfunction
endclass

class my_scoreboard extends uvm_subscriber#(my_packet);
    `uvm_component_utils(my_scoreboard)

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void write(my_packet t);
        my_packet local_pkt;
        local_pkt = new("local_pkt");
        // Safe: modifies local copy rather than original transaction 't'
        local_pkt.data = t.data;
        local_pkt.addr = t.addr;
    endfunction
endclass
