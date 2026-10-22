// SV-BUG-007: arguments passed in the wrong order.
class mem_model;
  bit [7:0] mem[int];
  function void write(int addr, bit [7:0] data);
    mem[addr] = data;
  endfunction
  function void fill(int addr, int len, bit [7:0] data);
    for (int i = 0; i < len; i++) write(addr + i, data);
  endfunction
endclass

class mem_user;
  mem_model m;
  function void run(int addr, int len, bit [7:0] data);
    m.write(data, addr); // VIOLATION
    m.fill(addr, data, len); // VIOLATION
  endfunction
endclass
