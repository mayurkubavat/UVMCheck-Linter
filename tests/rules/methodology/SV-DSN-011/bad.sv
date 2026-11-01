// SV-DSN-011: violations are marked.
`include "uvm_mock.svh"
class link_stats;
  int sent, received, dropped, retried, max_latency, min_latency;
  function void clear();
    sent = 0;
    received = 0;
  endfunction
endclass

class link_reporter;
  link_stats stats;
  string prefix;
  // Works almost only on stats' data: it belongs in link_stats.
  function string summary(); // VIOLATION
    int lost = stats.sent - stats.received - stats.dropped;
    if (stats.retried > stats.sent / 2) lost++;
    return $sformatf("%s %0d %0d %0d", prefix, lost, stats.max_latency, stats.min_latency);
  endfunction
endclass
