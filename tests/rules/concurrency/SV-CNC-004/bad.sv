// SV-CNC-004: two threads race on the same properties.
class link_model;
  int unsigned credits;
  bit [1:0] state;
  task run();
    fork // VIOLATION
      forever begin #3; credits++; end
      forever begin #5; credits--; end
    join_none
    fork // VIOLATION
      begin #1; state = 1; end
    join_none
    state = 2;
  endtask
endclass
