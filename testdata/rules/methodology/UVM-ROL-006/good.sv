// UVM-ROL-006 (VirtualInterfaceInChecker): pins stay in monitors and drivers
`include "uvm_mock.svh"

interface rol006_good_if(input logic clk);
    logic rst_n;
endinterface

class rol006_good_mon extends uvm_monitor;
    `uvm_component_utils(rol006_good_mon)
    virtual rol006_good_if vif;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class rol006_good_scb extends uvm_scoreboard;
    `uvm_component_utils(rol006_good_scb)
    int unsigned resets_seen;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void write_reset(bit asserted);
        if (asserted) resets_seen++;
    endfunction
endclass
