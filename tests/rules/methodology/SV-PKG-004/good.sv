// SV-PKG-004: imports that point down the stack (agent < env < test) are fine.
package tb_base_pkg;
  class uvm_component;
  endclass
  class uvm_object;
  endclass
  class uvm_sequence_item extends uvm_object;
  endclass
  class uvm_driver extends uvm_component;
  endclass
  class uvm_env extends uvm_component;
  endclass
  class uvm_test extends uvm_component;
  endclass
endpackage

package bus_item_pkg;
  import tb_base_pkg::*;
  class bus_item extends uvm_sequence_item;
  endclass
endpackage

package bus_agent_pkg;
  import tb_base_pkg::*;
  import bus_item_pkg::*;
  class bus_driver extends uvm_driver;
    bus_item req;
  endclass
endpackage

package bus_env_pkg;
  import tb_base_pkg::*;
  import bus_agent_pkg::*;
  class bus_env extends uvm_env;
    bus_driver drv;
  endclass
endpackage

package bus_test_pkg;
  import tb_base_pkg::*;
  import bus_env_pkg::*;
  class bus_test extends uvm_test;
    bus_env env;
  endclass
endpackage
