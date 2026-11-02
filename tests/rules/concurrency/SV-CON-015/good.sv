// SV-CON-015 (UnguardedBlockingWait): guarded waits, clocks, resets and monitors
`include "uvm_mock.svh"

interface con015_good_if(input logic clk);
    logic rst_n;
    logic req;
    logic ack;
    clocking cb @(posedge clk);
        input ack;
    endclocking
endinterface

class con015_good_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(con015_good_driver)
    virtual con015_good_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task drive_item();
        wait(vif.rst_n);              // reset release is not a handshake
        @(posedge vif.clk);           // clock edge
        @(vif.cb);                    // clocking-block event
        vif.req <= 1'b1;
        fork
            wait(vif.ack == 1'b1);
            begin
                #1000;
                $error("ack timeout");
            end
        join_any
        disable fork;
    endtask

    task wait_ack_with_timeout(int cycles);
        wait(vif.ack);                // the helper owns the timeout
    endtask
endclass

// Monitors wait on the bus forever by design.
class con015_good_monitor extends uvm_monitor;
    `uvm_component_utils(con015_good_monitor)
    virtual con015_good_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        forever @(posedge vif.ack) $display("ack");
    endtask
endclass
