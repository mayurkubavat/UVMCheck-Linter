// SV-RSC-001 good: released on every path; acquire-only helpers hand the lock to the caller.
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
    if (!tr.valid) begin
      bus_sem.put();
      return;
    end
    #10;
    bus_sem.put();
  endtask

  task acquire_bus();
    bus_sem.get();
  endtask

  task poll();
    if (!bus_sem.try_get()) return;
    #1;
    bus_sem.put();
  endtask

  task fatal_path(bus_item tr);
    bus_sem.get();
    if (tr == null) `uvm_fatal("DRV", "null item")
    bus_sem.put();
  endtask
endclass
