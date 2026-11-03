// Clauses 24-25: programs and interfaces. 24.3 program blocks, 25.3-25.5 interfaces with
// ports and modports, 25.5.4 modport expressions, 25.5.5 clocking in modports, 25.7 tasks
// in interfaces with import/export, 25.8 parameterized interfaces, 25.9 virtual interfaces.
interface ch25_bus #(parameter int W = 8) (input logic clk);
  logic [W-1:0] data;
  logic valid;
  clocking cb @(posedge clk);
    output data, valid;
  endclocking
  modport drv (clocking cb, import send);
  modport mon (input data, input valid, input .first_bit(data[0]));
  task automatic send(input logic [W-1:0] d);
    @(posedge clk);
    data <= d;
    valid <= 1'b1;
  endtask
endinterface

program automatic ch24_test(ch25_bus.drv bus);
  initial begin
    bus.send(8'h1);
    bus.cb.valid <= 1'b0;
  end
endprogram

class ch25_driver;
  virtual ch25_bus #(8) vif;
  function new(virtual ch25_bus #(8) vif);
    this.vif = vif;
  endfunction
endclass

module ch24_25_program_interface;
  logic clk = 0;
  ch25_bus #(8) bus (clk);
  ch24_test t (bus.drv);
  initial begin
    ch25_driver d = new(bus);
    $display("%b", d.vif.valid);
  end
endmodule
