// SV-BUG-007 good: matching order, unrelated names and named arguments.
class mem_model;
  bit [7:0] mem[int];
  function void write(int addr, bit [7:0] data);
    mem[addr] = data;
  endfunction
endclass

class mem_user;
  mem_model m;
  function void run(int addr, bit [7:0] data, int base);
    m.write(addr, data);
    m.write(base, 8'h0);
    m.write(.data(data), .addr(addr));
  endfunction
endclass
