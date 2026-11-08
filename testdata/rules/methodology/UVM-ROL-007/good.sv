// UVM-ROL-007 (ProceduralTimeoutInMonitor): the timing rule lives in the interface
`include "uvm_mock.svh"

interface rol007_good_if(input logic clk, input logic rst_n);
    logic req;
    logic ack;
    a_req_ack: assert property (@(posedge clk) disable iff (!rst_n) req |-> ##[1:4] ack)
        else $error("ack did not follow req within 4 cycles");
endinterface

class rol007_good_mon extends uvm_monitor;
    `uvm_component_utils(rol007_good_mon)
    virtual rol007_good_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        forever begin
            @(posedge vif.ack);
            fork
                @(posedge vif.req);
                repeat (2) @(posedge vif.clk);   // waiting, not reporting a timeout
            join_any
            disable fork;
        end
    endtask
endclass
