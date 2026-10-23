// Non-UVM helpers that rule tests use. Both the UVM stub (uvm_mock.svh) and the
// real-library shim (real_uvm/uvm_mock.svh) include this, so a test compiles the
// same way against either library.

`ifndef UVM_TEST_FIXTURES_SVH
`define UVM_TEST_FIXTURES_SVH

interface bus_if;
    logic clk;
    logic req;
    logic ack;
endinterface

`ifndef MOCK_BASE_DRIVER_DEFINED
`define MOCK_BASE_DRIVER_DEFINED
// Standard test mock drivers
class BaseDriver extends uvm_component;
    `uvm_component_utils(BaseDriver)
    function new(string name = "BaseDriver", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

class MyDriver extends BaseDriver;
    `uvm_component_utils(MyDriver)
    function new(string name = "MyDriver", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass
`endif

`endif // UVM_TEST_FIXTURES_SVH
