// SV-MET-003: too many positional arguments; pass the item instead.
`include "uvm_mock.svh"

typedef enum {SINGLE, INCR, WRAP} burst_e;

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  virtual task send(bit [31:0] addr, bit [31:0] data, int len, burst_e burst, // VIOLATION
                    bit lock, bit [2:0] prot, int id, int user);
  endtask
endclass

class fast_driver extends bus_driver;
  `uvm_component_utils(fast_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  // Override: the signature is forced by the base, reported there only.
  virtual task send(bit [31:0] addr, bit [31:0] data, int len, burst_e burst,
                    bit lock, bit [2:0] prot, int id, int user);
  endtask
endclass
