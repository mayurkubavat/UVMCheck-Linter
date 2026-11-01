// Violation Test Case: RAL-BLK-001 (RegBlockNeverLocked)
`include "uvm_mock.svh"

class ctrl_reg extends uvm_reg;
    `uvm_object_utils(ctrl_reg)
    function new(string name = "ctrl_reg");
        super.new(name);
    endfunction : new
endclass : ctrl_reg

class dma_block extends uvm_reg_block;   // sub-block: its parent locks it
    `uvm_object_utils(dma_block)
    ctrl_reg ctrl;
    function new(string name = "dma_block");
        super.new(name);
    endfunction : new
endclass : dma_block

class soc_block extends uvm_reg_block;   // VIOLATION: top block, never locked
    `uvm_object_utils(soc_block)
    dma_block dma;
    function new(string name = "soc_block");
        super.new(name);
    endfunction : new
    function void build();
        dma = dma_block::type_id::create("dma");
    endfunction : build
endclass : soc_block

class soc_env extends uvm_env;
    `uvm_component_utils(soc_env)
    soc_block regmodel;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction : new
    function void build_phase(uvm_phase phase);
        regmodel = soc_block::type_id::create("regmodel");
        regmodel.build();
    endfunction : build_phase
endclass : soc_env
