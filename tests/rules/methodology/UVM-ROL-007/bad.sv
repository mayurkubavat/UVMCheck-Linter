// UVM-ROL-007 (ProceduralTimeoutInMonitor): a handshake timeout counted by hand
`include "uvm_mock.svh"

interface rol007_if(input logic clk);
    logic req;
    logic ack;
endinterface

class rol007_mon extends uvm_monitor;
    `uvm_component_utils(rol007_mon)
    virtual rol007_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        forever begin
            @(posedge vif.req);
            fork
                @(posedge vif.ack);
                begin
                    repeat (4) @(posedge vif.clk);
                    `uvm_error("ACK_TMO", "ack did not follow req within 4 cycles")
                end
            join_any
            disable fork;
        end
    endtask
endclass
