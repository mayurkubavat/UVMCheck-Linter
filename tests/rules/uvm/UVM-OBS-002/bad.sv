// Test Case: UVM-OBS-002 (BlockingInObserver)
`include "uvm_mock.svh"

class obs_item extends uvm_sequence_item;
    `uvm_object_utils(obs_item)
    int seq_id;
    function new(string name = "obs_item"); super.new(name); endfunction
endclass

// A subscriber that forks a thread for every broadcast transaction.
class forking_coverage extends uvm_subscriber #(obs_item);
    `uvm_component_utils(forking_coverage)
    function new(string name, uvm_component parent); super.new(name, parent); endfunction

    function void write(obs_item t);
        fork
            sample_later(t);
        join_none
    endfunction

    task sample_later(obs_item t);
    endtask
endclass

`uvm_analysis_imp_decl(_exp)

// A scoreboard whose write_exp (behind uvm_analysis_imp_exp) is a task.
class task_scoreboard extends uvm_component;
    `uvm_component_utils(task_scoreboard)
    uvm_analysis_imp_exp #(obs_item, task_scoreboard) exp_imp;
    function new(string name, uvm_component parent);
        super.new(name, parent);
        exp_imp = new("exp_imp", this);
    endfunction

    task write_exp(obs_item t);
    endtask
endclass
