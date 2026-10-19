// Test for UVM-TLM-005: In-place mutation of transaction inside analysis write()
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
        // VIOLATION: In-place mutation of broadcasted transaction parameter 't'
        t.data = 32'hDEADBEEF;
    endfunction
endclass
