// SV-DSN-003: packet collection and error statistics share no data.
class bus_observer; // VIOLATION
  int unsigned pkt_count;
  int unsigned last_len;
  int unsigned err_hist[8];
  int unsigned err_total;
  function void collect(int unsigned len);
    pkt_count++;
    last_len = len;
  endfunction
  function int unsigned packets();
    return pkt_count;
  endfunction
  function void add_error(int code);
    err_hist[code % 8]++;
    err_total++;
  endfunction
  function int unsigned errors();
    return err_total;
  endfunction
endclass
