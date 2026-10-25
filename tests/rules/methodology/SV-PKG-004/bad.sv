// SV-PKG-004: violations are marked.
// The UVM stub lives in $unit, which packages cannot see, so the base classes are declared
// in a local package here; their uvm_* names give the classes below their roles.
package tb_base_pkg;
  class uvm_component;
  endclass
  class uvm_driver extends uvm_component;
  endclass
  class uvm_env extends uvm_component;
  endclass
  class uvm_test extends uvm_component;
  endclass
endpackage

package bus_env_pkg;
  import tb_base_pkg::*;
  class bus_env extends uvm_env;
  endclass
endpackage

package bus_test_pkg;
  import tb_base_pkg::*;
  class bus_test extends uvm_test;
  endclass
endpackage

package bus_agent_pkg;
  import tb_base_pkg::*;
  import bus_env_pkg::*; // VIOLATION
  class bus_driver extends uvm_driver;
    bus_env env;
  endclass
endpackage

package top_env_pkg;
  import tb_base_pkg::*;
  import bus_test_pkg::bus_test; // VIOLATION
  class top_env extends uvm_env;
    bus_test t;
  endclass
endpackage
