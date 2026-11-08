// SV-API-001 good: const ref, written refs and virtual methods.
class q_utils;
  function int sum(const ref int q[$]);
    int s = 0;
    foreach (q[i]) s += q[i];
    return s;
  endfunction
  function void add(ref int q[$], int v);
    q.push_back(v);
  endfunction
  function void clear(ref int q[$]);
    q = {};
  endfunction
  function void bump(ref int x);
    x++;
  endfunction
  virtual function int peek(ref int q[$]);
    return q[0];
  endfunction
endclass
