// SV-MET-004 good: early exits keep it flat.
class flat_model;
  int q[$];
  function void scan(int n);
    for (int i = 1; i < n; i++) drain_all();
  endfunction
  function void drain_all();
    foreach (q[j]) while (q[j] > 0) q[j]--;
  endfunction
endclass
