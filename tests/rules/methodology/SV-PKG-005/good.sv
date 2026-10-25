// SV-PKG-005: dependencies point towards more stable packages.
package bus_types_pkg;
  typedef int addr_t;
endpackage

package bus_common_pkg;
  import bus_types_pkg::*;
  typedef addr_t common_t;
endpackage

package bus_a_pkg;
  import bus_common_pkg::*;
  import bus_types_pkg::*;
  typedef common_t a_t;
  typedef addr_t a_addr_t;
endpackage

package bus_b_pkg;
  import bus_common_pkg::*;
  typedef common_t b_t;
endpackage
