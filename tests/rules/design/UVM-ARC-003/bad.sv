// UVM-ARC-003: bus_item.svh is included into two packages, creating two bus_item types.
package axi_pkg;
  `include "bus_item.svh"
endpackage

package soc_pkg; // VIOLATION
  `include "bus_item.svh"
endpackage
