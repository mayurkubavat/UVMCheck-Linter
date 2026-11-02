// UVM-ROL-006 (VirtualInterfaceInChecker): pin access in checkers
`include "uvm_mock.svh"

interface rol006_if(input logic clk);
    logic rst_n;
    logic [7:0] data;
endinterface

class rol006_scb extends uvm_scoreboard;
    `uvm_component_utils(rol006_scb)
    virtual rol006_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        @(negedge vif.rst_n);
    endtask
endclass

class rol006_cov extends uvm_subscriber #(uvm_sequence_item);
    `uvm_component_utils(rol006_cov)
    virtual rol006_if bus_vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void write(uvm_sequence_item t);
    endfunction
endclass
