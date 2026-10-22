// SV-MET-006: a scoreboard with 31 properties.
`include "uvm_mock.svh"

class soc_scoreboard extends uvm_scoreboard; // VIOLATION
  int unsigned f0;
  int unsigned f1;
  int unsigned f2;
  int unsigned f3;
  int unsigned f4;
  int unsigned f5;
  int unsigned f6;
  int unsigned f7;
  int unsigned f8;
  int unsigned f9;
  int unsigned f10;
  int unsigned f11;
  int unsigned f12;
  int unsigned f13;
  int unsigned f14;
  int unsigned f15;
  int unsigned f16;
  int unsigned f17;
  int unsigned f18;
  int unsigned f19;
  int unsigned f20;
  int unsigned f21;
  int unsigned f22;
  int unsigned f23;
  int unsigned f24;
  int unsigned f25;
  int unsigned f26;
  int unsigned f27;
  int unsigned f28;
  int unsigned f29;
  int unsigned f30;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
