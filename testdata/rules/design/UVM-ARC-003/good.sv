// UVM-ARC-003 good: one package owns the file, the other imports it.
package axi_pkg;
  `include "bus_item.svh"
endpackage

package soc_pkg;
  import axi_pkg::*;
  class soc_item extends bus_item;
  endclass
endpackage
