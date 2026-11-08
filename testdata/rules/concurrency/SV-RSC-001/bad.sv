// SV-RSC-001: an early return leaves the bus semaphore and the sequencer lock held.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  bit valid;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  semaphore bus_sem;
  function new(string name, uvm_component parent);
    super.new(name, parent);
    bus_sem = new(1);
  endfunction

  task drive(bus_item tr);
    bus_sem.get();
    if (!tr.valid) return; // VIOLATION
    #10;
    bus_sem.put();
  endtask

  task poll(bus_item tr);
    if (!bus_sem.try_get()) return;
    forever begin
      if (tr.valid) break;
      #1;
    end
    if (tr == null) return; // VIOLATION
    bus_sem.put();
  endtask
endclass

class excl_seq extends uvm_sequence #(bus_item);
  `uvm_object_utils(excl_seq)
  bit abort;
  function new(string name = "excl_seq");
    super.new(name);
  endfunction
  virtual task body();
    lock(m_sequencer);
    repeat (4) begin
      if (abort) return; // VIOLATION
      #10;
    end
    unlock(m_sequencer);
  endtask
endclass
