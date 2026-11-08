// SV-PKG-001: one class per included file, named after it; classes in the package file itself
// and in files compiled on their own are not this rule's concern.
`include "uvm_mock.svh"
package mon_pkg;
  `include "bus_monitor.svh"
  class local_helper;
  endclass
  class other_helper;
  endclass
endpackage

class unit_helper;
endclass
