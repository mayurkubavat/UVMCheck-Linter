// SV-CON-015 (UnguardedBlockingWait): handshake waits with no timeout
`include "uvm_mock.svh"

interface con015_if(input logic clk);
    logic       rst_n;
    logic       req;
    logic       ack;
    logic       irq_done;
    clocking cb @(posedge clk);
        input ack;
    endclocking
endinterface

class con015_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(con015_driver)
    virtual con015_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task drive_item();
        vif.req <= 1'b1;
        wait(vif.ack == 1'b1);
        @(posedge vif.irq_done);
        wait(vif.cb.ack);
    endtask
endclass
