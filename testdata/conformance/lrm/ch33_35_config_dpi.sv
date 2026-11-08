// Clauses 33 and 35: configurations and DPI. 33.4 config blocks with design, default liblist
// and instance rules, 35.4-35.8 DPI-C imports (pure, context, tasks, open arrays) and exports.
module ch35_dpi;
  import "DPI-C" pure function int c_add(input int a, input int b);
  import "DPI-C" context task c_wait(input int cycles);
  import "DPI-C" function void c_sum(input int vals [], output int total);
  export "DPI-C" function sv_hello;
  function void sv_hello();
    $display("hello");
  endfunction
  initial begin
    int t;
    int v [] = '{1, 2};
    c_sum(v, t);
    c_wait(c_add(t, 1));
  end
endmodule

config ch33_cfg;
  design work.ch35_dpi;
  default liblist work;
endconfig
