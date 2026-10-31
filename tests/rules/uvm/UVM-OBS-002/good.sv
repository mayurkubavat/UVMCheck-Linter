// Compliant Clean Test Case: UVM-OBS-002 (BlockingInObserver)
`include "uvm_mock.svh"

class obs_item extends uvm_sequence_item;
    `uvm_object_utils(obs_item)
    int seq_id;
    function new(string name = "obs_item"); super.new(name); endfunction
endclass

class clean_coverage extends uvm_subscriber #(obs_item);
    `uvm_component_utils(clean_coverage)
    int last_id;
    function new(string name, uvm_component parent); super.new(name, parent); endfunction
    function void write(obs_item t);
        last_id = t.seq_id;
    endfunction
endclass

`uvm_analysis_imp_decl(_act)

class clean_scoreboard extends uvm_component;
    `uvm_component_utils(clean_scoreboard)
    uvm_analysis_imp_act #(obs_item, clean_scoreboard) act_imp;
    obs_item pending[$];
    function new(string name, uvm_component parent);
        super.new(name, parent);
        act_imp = new("act_imp", this);
    endfunction
    function void write_act(obs_item t);
        pending.push_back(t);
    endfunction
endclass

// A driver's own write_* tasks are not analysis callbacks.
class reg_driver extends uvm_driver #(obs_item);
    `uvm_component_utils(reg_driver)
    function new(string name, uvm_component parent); super.new(name, parent); endfunction
    task write_reg(int addr, int data);
        #10;
    endtask
    task write(int data);
        #1;
    endtask
endclass
