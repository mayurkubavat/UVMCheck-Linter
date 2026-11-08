// SV-RDB-002: protocol limits written as bare numbers.
class eth_checker;
  int unsigned drops;
  function void check(int unsigned len);
    if (len > 1518) drops++; // VIOLATION
    for (int i = 0; i < 64; i++) begin // VIOLATION
      if (len == 1518) drops--;
    end
  endfunction
endclass
