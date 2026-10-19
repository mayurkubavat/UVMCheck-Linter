`include "uvm_mock.svh"

// Non-compliant driver: does not start with cdrv_
class my_driver extends uvm_component;
  `uvm_component_utils(my_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

// Non-compliant scoreboard: does not end with _scb
class test_scoreboard extends uvm_component;
  `uvm_component_utils(test_scoreboard)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

// Compliant driver: starts with cdrv_
class cdrv_axi_driver extends uvm_component;
  `uvm_component_utils(cdrv_axi_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

// Compliant scoreboard: ends with _scb
class packet_checker_scb extends uvm_component;
  `uvm_component_utils(packet_checker_scb)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
