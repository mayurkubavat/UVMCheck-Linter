// SV-DSN-011: no violations.
`include "uvm_mock.svh"
class link_stats;
  int sent, received, dropped, retried, max_latency, min_latency;
  function int lost();
    lost = sent - received - dropped;
    if (retried > sent / 2) lost++;
  endfunction
endclass

class link_reporter;
  link_stats stats;
  string prefix;
  function string summary();
    return $sformatf("%s %0d %0d %0d", prefix, stats.lost(), stats.max_latency, stats.min_latency);
  endfunction
endclass

// Transactions are data a component is meant to read.
class dsn_item extends uvm_sequence_item;
  `uvm_object_utils(dsn_item)
  rand bit [7:0] a, b, c, d, e, f, g;
  function new(string name = "dsn_item");
    super.new(name);
  endfunction
endclass

class dsn_predictor extends uvm_component;
  `uvm_component_utils(dsn_predictor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function int predict(dsn_item t);
    return t.a + t.b + t.c + t.d + t.e + t.f + t.g;
  endfunction
endclass
