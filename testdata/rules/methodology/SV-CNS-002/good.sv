// SV-CNS-002: macros that need the preprocessor are fine.
`include "uvm_mock.svh"
`define BUS_ERR(msg) `uvm_error("BUS", msg)
`define FIELD_DECL(T, name) rand T name; constraint name``_c { name != 0; }
`define TRACE(x) $display(`"x = %0d`", x); $display("at %s:%0d", `__FILE__, `__LINE__);
`define CREATE(T, h) h = T::type_id::create(`"h`", this);
`define RETURN_IF_NULL(h) if (h == null) return; $display("ok");
`define ONE_CALL(x) $display("%0d", x)
`define FLOP(d, q, clk) \
  always_ff @(posedge clk) begin \
    q <= d; \
  end
`ifndef ASSERT_CTRL
`define ASSERT_CTRL(label, value) begin $display(label); $display("%0d", value); end
`endif

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  `FIELD_DECL(int, addr)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void report(bus_item it);
    `RETURN_IF_NULL(it)
    `TRACE(it.addr)
    `ONE_CALL(it.addr);
    `BUS_ERR("bad")
  endfunction
endclass
