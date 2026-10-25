// SV-PKG-002: imports inside modules and in the package file itself, and explicit imports.
`include "uvm_mock.svh"
package bus_types_pkg;
  typedef int addr_t;
endpackage

package bus_pkg;
  import bus_types_pkg::*;
  class bus_item;
    addr_t addr;
  endclass
endpackage

import bus_types_pkg::addr_t;

module tb;
  import bus_pkg::*;
  bus_item it;
  addr_t a;
endmodule
