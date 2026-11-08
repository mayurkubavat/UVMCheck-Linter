// SV-PKG-005: the violation is marked. bus_common_pkg is used by three packages (stable) but
// depends on bus_debug_pkg, which itself depends on two others (unstable).
package bus_log_pkg;
  typedef int log_t;
endpackage

package bus_cfg_pkg;
  typedef int cfg_t;
endpackage

package bus_debug_pkg;
  import bus_log_pkg::*;
  import bus_cfg_pkg::*;
  typedef log_t dbg_t;
  typedef cfg_t dbg_cfg_t;
endpackage

package bus_common_pkg;
  import bus_debug_pkg::*; // VIOLATION
  typedef dbg_t common_t;
endpackage

package bus_a_pkg;
  import bus_common_pkg::*;
  typedef common_t a_t;
endpackage

package bus_b_pkg;
  import bus_common_pkg::*;
  typedef common_t b_t;
endpackage

package bus_c_pkg;
  import bus_common_pkg::*;
  typedef common_t c_t;
endpackage
