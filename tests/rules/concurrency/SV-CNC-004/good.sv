// SV-CNC-004 good: separate properties per thread, a semaphore, and queue appends.
class link_model;
  int unsigned tx_credits;
  int unsigned rx_credits;
  int unsigned shared_count;
  int log_q[$];
  semaphore lock = new(1);
  task run();
    fork
      forever begin #3; tx_credits++; log_q.push_back(1); end
      forever begin #5; rx_credits++; log_q.push_back(2); end
    join_none
    fork
      begin lock.get(); shared_count++; lock.put(); end
      begin lock.get(); shared_count--; lock.put(); end
    join
  endtask
endclass
