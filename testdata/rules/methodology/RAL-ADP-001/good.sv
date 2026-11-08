// RAL-ADP-001: compliant code.
`include "uvm_mock.svh"
class bus_txn extends uvm_sequence_item;
  `uvm_object_utils(bus_txn)
  bit write;
  bit [31:0] addr;
  bit [31:0] data;
  bit error;
  function new(string name = "bus_txn");
    super.new(name);
  endfunction
endclass

class bus_adapter extends uvm_reg_adapter;
  `uvm_object_utils(bus_adapter)
  function new(string name = "bus_adapter");
    super.new(name);
  endfunction
  virtual function uvm_sequence_item reg2bus(const ref uvm_reg_bus_op rw);
    bus_txn it = bus_txn::type_id::create("it");
    it.write = (rw.kind == UVM_WRITE);
    it.addr = rw.addr;
    it.data = rw.data;
    return it;
  endfunction
  virtual function void bus2reg(uvm_sequence_item bus_item, ref uvm_reg_bus_op rw);
    bus_txn it;
    if (!$cast(it, bus_item)) return;
    rw.kind = it.write ? UVM_WRITE : UVM_READ;
    rw.addr = it.addr;
    rw.data = it.data;
    rw.status = it.error ? UVM_NOT_OK : UVM_IS_OK;
  endfunction
endclass
