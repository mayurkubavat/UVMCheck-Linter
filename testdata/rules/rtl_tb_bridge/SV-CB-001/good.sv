// Compliant Clean Test Case: SV-CB-001 (ClockingBlockOutputReadSkew)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_clockingblockoutputreadskew extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Driving a clocking block output is what it is for: the target of the
// drive is written, not read.
interface cb001_bus_if;
  logic clk;
  logic [7:0] data;
  clocking cb @(posedge clk);
    output data;
  endclocking
endinterface

class cb001_driver;
  virtual cb001_bus_if vif;
  task drive(bit [7:0] value);
    // One beat, then idle.
    vif.cb.data <= value;
    @(vif.cb);
    vif.cb.data <= 8'h00;
  endtask
endclass
