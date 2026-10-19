// Test for UVM-PHS-010: Accessing sibling component configuration during build_phase
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

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    virtual function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION: Accessing sibling agent_a through parent during build_phase
        if (parent.a.cfg != null) begin
        end
    endfunction
endclass
