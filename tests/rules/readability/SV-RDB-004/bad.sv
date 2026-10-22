// SV-RDB-004: hooks in a base driver with no description.
virtual class bus_base_driver;
  int unsigned sent;
  virtual task pre_drive(int tr); // VIOLATION
  endtask
  pure virtual task drive_item(int tr); // VIOLATION
  task run(int tr);
    pre_drive(tr);
    drive_item(tr);
  endtask
endclass
