// Test: UVM-FAC-001 (Direct instantiation of uvm_component via new)
`include "uvm_mock.svh"

class my_driver extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass

class my_env extends uvm_component;
    my_driver drv;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // expected-error: UVM-FAC-001
        drv = new("drv", this);
    endfunction
endclass
