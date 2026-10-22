// SV-DSN-010: queries that change state.
class id_pool;
  int m_id;
  int m_q[$];
  function int get_id(); // VIOLATION
    return m_id++;
  endfunction
  function bit is_empty(); // VIOLATION
    void'(m_q.pop_front());
    return m_q.size() == 0;
  endfunction
endclass
