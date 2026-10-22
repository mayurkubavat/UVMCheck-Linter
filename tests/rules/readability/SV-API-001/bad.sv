// SV-API-001: ref arguments that are only read.
class q_utils;
  function int sum(ref int q[$]); // VIOLATION
    int s = 0;
    foreach (q[i]) s += q[i];
    return s;
  endfunction
  function bit has(ref int q[$], input int v); // VIOLATION
    return q.size() > 0 && q[0] == v;
  endfunction
endclass
