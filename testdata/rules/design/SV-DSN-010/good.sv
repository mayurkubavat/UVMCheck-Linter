// SV-DSN-010 good: pure queries and the get_next_ exception.
class id_pool;
  int m_id;
  int m_q[$];
  function int get_id();
    return m_id;
  endfunction
  function int get_next_id();
    return m_id++;
  endfunction
  function bit has_items();
    int n = m_q.size();
    return n > 0;
  endfunction
endclass
