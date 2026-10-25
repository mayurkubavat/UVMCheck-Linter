// SV-PKG-003: every import supplies something that is used.
`include "uvm_mock.svh"
package bus_types_pkg;
  typedef int addr_t;
  typedef enum bit {RD, WR} dir_e;
endpackage

package util_pkg;
  function automatic int clog2(int v);
    return $clog2(v);
  endfunction
endpackage

package bus_pkg;
  import bus_types_pkg::*;
  import util_pkg::clog2;
  class bus_item;
    addr_t addr;
    dir_e dir = WR;
    int bits = clog2(16);
  endclass
endpackage

module tb;
  import bus_pkg::*;
  import bus_types_pkg::RD;
  bus_item it;
  initial if (it.dir == RD) $display("rd");
endmodule
