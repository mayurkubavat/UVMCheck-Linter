// Compliant: UVM-FAC-001 (Factory create used instead of new)
`include "uvm_mock.svh"

class my_driver extends uvm_component;
    `uvm_component_utils(my_driver)

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class my_env extends uvm_component;
    `uvm_component_utils(my_env)
    my_driver drv;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // Compliant: factory create
        drv = my_driver::type_id::create("drv", this);
    endfunction
endclass
