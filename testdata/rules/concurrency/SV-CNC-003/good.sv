// SV-CNC-003 good: event waits, timed counters and clock generators.
interface poll_if;
  logic ready;
  logic clk;
endinterface

class bus_waiter;
  virtual poll_if vif;
  bit done;
  int unsigned cycles;
  task wait_ready();
    wait (vif.ready);
    while (!done) @(posedge vif.clk);
  endtask
  task count_cycles();
    while (cycles < 10) begin
      #10;
      cycles++;
    end
  endtask
endclass

module tb;
  logic clk;
  initial forever #5 clk = ~clk;
endmodule
