package p;
class base;
  int m_a;
  rand int m_r;
  local int m_w;
  static int m_s = 1;
  int m_ro;
  my_t m_t;
  int arr[4];
  function new();
    m_a = 0;
  endfunction
  function void touch(int v);
    m_w = v;
    m_w++;
    if (!m_ro) m_a += 1;
    arr[0] = this.m_a;
    pkg::fn(m_t.x);
  endfunction
  function void empty_f();
  endfunction
  task empty_t(); ; endtask
  virtual function void vf(); endfunction
  extern function void ext();
  pure virtual function void pv();
endclass
function void base::ext();
endfunction
endpackage
