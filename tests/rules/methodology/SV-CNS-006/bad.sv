// SV-CNS-006: violations are marked.
`include "uvm_mock.svh"
package bus_pkg;
  typedef enum {READ, WRITE} dir_e; // VIOLATION
endpackage

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  typedef enum {OKAY, ERROR, RETRY} resp_e; // VIOLATION
  rand resp_e resp;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

interface bus_ctl_if;
  enum {IDLE, BUSY} state; // VIOLATION
endinterface
