// SV-DUP-005: two agents that really differ.
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

package spi_agent_pkg;
  import tb_base_pkg::*;

  class spi_item extends uvm_sequence_item;
    bit [7:0] cmd;
    bit [31:0] payload;
    bit cs_n;
    function string convert2string();
      return $sformatf("cmd=%h payload=%h", cmd, payload);
    endfunction
  endclass

  class spi_driver extends uvm_driver;
    int frames;
    function void drive(spi_item t);
      t.cs_n = 0;
      for (int i = 0; i < 32; i++) frames = frames + t.payload[i];
      t.cs_n = 1;
    endfunction
  endclass

  class spi_monitor extends uvm_monitor;
    int seen;
    function void sample(spi_item t);
      if (!t.cs_n) seen = seen + t.cmd;
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
