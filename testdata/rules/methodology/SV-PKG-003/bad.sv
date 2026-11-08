// SV-PKG-003: violations are marked.
`include "uvm_mock.svh"
package bus_types_pkg;
  typedef int addr_t;
  typedef int data_t;
endpackage

package util_pkg;
  function automatic int clog2(int v);
    return $clog2(v);
  endfunction
endpackage

package bus_pkg;
  import bus_types_pkg::*;
  import util_pkg::*; // VIOLATION
  import bus_types_pkg::data_t; // VIOLATION
  class bus_item;
    addr_t addr;
  endclass
endpackage

module tb;
  import util_pkg::*; // VIOLATION
  import bus_pkg::*;
  bus_item it;
endmodule
