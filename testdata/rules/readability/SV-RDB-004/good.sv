// SV-RDB-004 good: documented hooks, overrides and non-virtual helpers.
virtual class bus_base_driver;
  int unsigned sent;
  // Called before each item is driven; override to modify or delay the item.
  virtual task pre_drive(int tr);
  endtask
  /* Drives one item on the bus. Must return after the last beat. */
  pure virtual task drive_item(int tr);
  task run(int tr);
    pre_drive(tr);
    drive_item(tr);
  endtask
endclass

class axi_driver extends bus_base_driver;
  virtual task drive_item(int tr);
    sent++;
  endtask
endclass
