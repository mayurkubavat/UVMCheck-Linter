// SV-DED-015 (NeverSetClassParameter): parameters nobody sets
`include "uvm_mock.svh"

class ded015_agent #(int MAX_SLAVES = 1, int WIDTH = 32) extends uvm_object;
    bit [WIDTH-1:0] data_q[$];
    function new(string name = "ded015_agent");
        super.new(name);
    endfunction
endclass

class ded015_env extends uvm_object;
    ded015_agent #(.WIDTH(64)) wide_agt;
    ded015_agent narrow_agt;
    function new(string name = "ded015_env");
        super.new(name);
    endfunction
endclass
