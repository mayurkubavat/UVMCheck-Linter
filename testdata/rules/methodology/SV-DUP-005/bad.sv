// SV-DUP-005: the spi agent is the apb agent copied and renamed.
package tb_base_pkg;
  class uvm_object;
  endclass
  class uvm_component;
  endclass
  class uvm_driver extends uvm_component;
  endclass
  class uvm_monitor extends uvm_component;
  endclass
  class uvm_agent extends uvm_component;
  endclass
  class uvm_sequence_item extends uvm_object;
  endclass
endpackage

package apb_agent_pkg;
  import tb_base_pkg::*;

  class apb_item extends uvm_sequence_item;
    bit [31:0] addr;
    bit [31:0] data;
    function string convert2string();
      return $sformatf("addr=%h data=%h", addr, data);
    endfunction
  endclass

  class apb_driver extends uvm_driver;
    int sent;
    function void drive(apb_item t);
      if (t.addr[1:0] != 0) sent = sent - 1;
      sent = sent + 1;
      t.data = t.data ^ t.addr;
    endfunction
  endclass

  class apb_monitor extends uvm_monitor;
    int seen;
    function void sample(apb_item t);
      seen = seen + 1;
      if (t.data == 0) seen = seen - 1;
      t.addr = t.addr & 32'hFFFC;
    endfunction
  endclass

  class apb_agent extends uvm_agent;
    apb_driver drv;
    apb_monitor mon;
    function void build();
      drv = new();
      mon = new();
      
    endfunction
  endclass
endpackage

package spi_agent_pkg; // VIOLATION
  import tb_base_pkg::*;

  class spi_item extends uvm_sequence_item;
    bit [31:0] addr;
    bit [31:0] data;
    function string convert2string();
      return $sformatf("addr=%h data=%h", addr, data);
    endfunction
  endclass

  class spi_driver extends uvm_driver;
    int sent;
    function void drive(spi_item t);
      if (t.addr[1:0] != 0) sent = sent - 1;
      sent = sent + 1;
      t.data = t.data ^ t.addr;
    endfunction
  endclass

  class spi_monitor extends uvm_monitor;
    int seen;
    function void sample(spi_item t);
      seen = seen + 1;
      if (t.data == 0) seen = seen - 1;
      t.addr = t.addr & 32'hFFFC;
    endfunction
  endclass

  class spi_agent extends uvm_agent;
    spi_driver drv;
    spi_monitor mon;
    function void build();
      drv = new();
      mon = new();
      
    endfunction
  endclass
endpackage
