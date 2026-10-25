// SV-PKG-002: violations are marked; one is in the included helper.
`include "uvm_mock.svh"
package bus_types_pkg;
  typedef int addr_t;
endpackage

package bus_pkg;
  import bus_types_pkg::*;
  `include "bus_item_hdr.svh"
endpackage

import bus_pkg::*; // VIOLATION

module tb;
  bus_item it;
endmodule
