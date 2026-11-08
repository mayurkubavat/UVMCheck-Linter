// Test for UVM-VSEQ-001: All sub-sequencer handles in virtual sequencer are properly wired
`include "uvm_mock.svh"

class my_agent_sequencer extends uvm_sequencer;
    `uvm_component_utils(my_agent_sequencer)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class my_virtual_sequencer extends uvm_sequencer;
    `uvm_component_utils(my_virtual_sequencer)

    my_agent_sequencer ahb_seqr;
    my_agent_sequencer apb_seqr;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class my_env extends uvm_env;
    `uvm_component_utils(my_env)

    my_virtual_sequencer v_seqr;
    my_agent_sequencer ahb_agent_seqr;
    my_agent_sequencer apb_agent_seqr;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        v_seqr.ahb_seqr = ahb_agent_seqr;
        v_seqr.apb_seqr = apb_agent_seqr; // Both sub-sequencers properly connected
    endfunction
endclass
