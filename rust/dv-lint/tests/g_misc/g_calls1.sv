class uvm_object; endclass
class uvm_phase extends uvm_object;
  function void raise_objection(uvm_object o); endfunction
  function void drop_objection(uvm_object o); endfunction
endclass
class uvm_component extends uvm_object;
  function new(string name, uvm_component parent); endfunction
  function bit uvm_report_enabled(int v); return 1; endfunction
  function void uvm_report_info(string id, string message, int verbosity = 300, string filename = "", int line = 0); endfunction
  function void uvm_report_warning(string id, string message, int verbosity = 200); endfunction
endclass
class my_comp extends uvm_component;
  bit [127:0] aes_key;
  int seed;
  int arr[];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    phase.raise_objection(this);
    $display("key=%h", aes_key);
    $display("plain", seed);
    $write("%0d %0d", seed, (aes_key));
    uvm_report_info("ID", $sformatf("v=%0d", seed));
    uvm_report_info("ID", $sformatf("v=%0d", seed), 400);
    uvm_report_info("ID", $sformatf("v=%0d", seed), 100);
    uvm_report_warning("ID", $sformatf("v=%0d", seed));
    if (uvm_report_enabled(300)) uvm_report_info("ID", $sformatf("v=%0d", seed), 300);
    #0;
    #(0) arr[0] = 1;
    #2;
    for (int i = 0; i < 4; i++) arr = new[arr.size() + 1](arr);
    while (seed < 3) begin arr = new[2](arr); seed++; end
    repeat (2) arr = new[3](arr);
    #100;
    phase.drop_objection(this);
  endtask
  function void report_phase(uvm_phase phase);
    $readmemh("/home/user/data.hex", arr);
    $readmemb("rel/data.bin", arr);
    void'($fopen("C:\\tmp\\x.log"));
    if (seed > 3) $finish;
    $stop();
  endfunction
  extern task ext_t(uvm_phase phase);
endclass
task my_comp::ext_t(uvm_phase phase);
  #5 phase.drop_objection(this);
  $exit;
endtask
class gen #(type T = int);
  T secret_v;
  task t(); #0; $display("%0d", secret_v); endtask
endclass
class gen_user;
  gen #(byte) g1;
  gen #(shortint) g2;
endclass
