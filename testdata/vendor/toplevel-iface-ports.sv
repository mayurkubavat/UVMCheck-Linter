// slang: --allow-toplevel-iface-ports
// A top-level module with an interface port, left unconnected (23.3.3.4).
interface bus_if;
  logic v;
endinterface
module probe(bus_if b);
endmodule
