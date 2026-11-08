// SV-CNS-008: return values, try-style status functions, ref/inout, tasks and overrides are fine.
`include "uvm_mock.svh"
virtual class base_model extends uvm_component;
    function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  pure virtual function void lookup(int addr, output int data);
endclass

class bus_model extends base_model;
  `uvm_component_utils(bus_model)
  int q[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function int predict(int addr);
    return addr * 2;
  endfunction
  function bit try_pop(output int data);
    if (q.size() == 0) return 0;
    data = q.pop_front();
    return 1;
  endfunction
  function void bump(ref int count, inout int total);
    count++;
    total++;
  endfunction
  task get_next(output int data);
    data = 1;
  endtask
  function void lookup(int addr, output int data);
    data = addr;
  endfunction
endclass
