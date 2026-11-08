// Clause 15: interprocess synchronization. 15.3 semaphores, 15.4 mailboxes (typed),
// 15.5 named events, triggered, wait_order, event merging.
module ch15_ipc;
  semaphore sem = new(1);
  mailbox #(int) mb = new(4);
  event e1, e2;
  initial begin
    int v;
    sem.get(1);
    void'(sem.try_get(1));
    sem.put(1);
    mb.put(5);
    if (mb.try_get(v)) $display("%0d", v);
    fork
      -> e1;
      ->> e2;
    join
    wait (e1.triggered);
    wait_order (e1, e2) else $display("out of order");
  end
endmodule
