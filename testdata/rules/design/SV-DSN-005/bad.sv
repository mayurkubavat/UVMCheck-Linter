// SV-DSN-005: four user-defined base tests above uvm_test.
`include "uvm_mock.svh"

class company_base_test extends uvm_test;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
class proj_base_test extends company_base_test;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
class soc_base_test extends proj_base_test;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
class base_test extends soc_base_test;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class smoke_test extends base_test; // VIOLATION
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
