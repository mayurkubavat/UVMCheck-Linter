// Test for UVM-PHS-010: Accessing configurations via uvm_config_db or in connect_phase
`include "uvm_mock.svh"

class agent_a_cfg extends uvm_object;
    int mode;
endclass

class agent_a extends uvm_component;
    `uvm_component_utils(agent_a)
    agent_a_cfg cfg;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class my_env;
    agent_a a;
endclass

class agent_b extends uvm_component;
    `uvm_component_utils(agent_b)
    my_env parent;
    agent_a_cfg local_cfg;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // Clean: uses config_db rather than sibling pointer dereferencing
        void'(uvm_config_db#(agent_a_cfg)::get(this, "", "cfg", local_cfg));
    endfunction

    virtual function void connect_phase(uvm_phase phase);
        super.connect_phase(phase);
        // Clean: cross-component wiring in connect_phase is valid
        if (parent.a.cfg != null) begin
        end
    endfunction
endclass
