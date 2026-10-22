// SV-RDB-002 good: named constants, 0/1 and report text.
`include "uvm_mock.svh"

class eth_checker extends uvm_component;
  localparam int unsigned MAX_FRAME = 1518;
  int unsigned drops;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void check(int unsigned len);
    if (len > MAX_FRAME) drops++;
    if (drops == 1) drops = 0;
    `uvm_info("CHK", $sformatf("len %0d of %0d", len, 1518), UVM_LOW)
  endfunction
endclass
