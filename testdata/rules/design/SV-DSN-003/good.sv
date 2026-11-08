// SV-DSN-003 good: every method works on the same counters.
class bus_observer;
  int unsigned pkt_count;
  int unsigned err_total;
  function void collect(bit err);
    pkt_count++;
    if (err) err_total++;
  endfunction
  function real error_rate();
    return pkt_count ? real'(err_total) / pkt_count : 0.0;
  endfunction
  function void clear();
    pkt_count = 0;
    err_total = 0;
  endfunction
endclass
