// UVM-PAT-003: no violations.
`include "uvm_mock.svh"
interface pat_bus_if;
  logic [7:0] data0, data1, data2, data3;
endinterface
class pat_item extends uvm_sequence_item;
  `uvm_object_utils(pat_item)
  rand bit [7:0] data[32];
  function new(string name = "pat_item");
    super.new(name);
  endfunction
endclass
class hooked_driver extends uvm_driver #(pat_item);
  `uvm_component_utils(hooked_driver)
  virtual pat_bus_if vif;
  pat_item req;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  // A hook tests can override.
  virtual task pre_drive(pat_item item);
  endtask
  task run_phase(uvm_phase phase);
    forever begin
      #10;
      pre_drive(req);
      vif.data0 <= req.data[0];
      vif.data1 <= req.data[1];
      vif.data2 <= req.data[2];
      vif.data3 <= req.data[3];
      vif.data0 <= req.data[4];
      vif.data1 <= req.data[5];
      vif.data2 <= req.data[6];
      vif.data3 <= req.data[7];
      vif.data0 <= req.data[8];
      vif.data1 <= req.data[9];
      vif.data2 <= req.data[10];
      vif.data3 <= req.data[11];
      vif.data0 <= req.data[12];
      vif.data1 <= req.data[13];
      vif.data2 <= req.data[14];
      vif.data3 <= req.data[15];
      vif.data0 <= req.data[16];
      vif.data1 <= req.data[17];
      vif.data2 <= req.data[18];
      vif.data3 <= req.data[19];
      vif.data0 <= req.data[20];
      vif.data1 <= req.data[21];
      vif.data2 <= req.data[22];
      vif.data3 <= req.data[23];
      vif.data0 <= req.data[24];
      vif.data1 <= req.data[25];
      vif.data2 <= req.data[26];
      vif.data3 <= req.data[27];
    end
  endtask
endclass

class short_monitor extends uvm_monitor;
  `uvm_component_utils(short_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    #10;
  endtask
endclass
