// SV-DED-015 (NeverSetClassParameter): every parameter is used by some specialization
`include "uvm_mock.svh"

class ded015_good_fifo #(type T = int, int DEPTH = 8) extends uvm_object;
    T items[$:DEPTH];
    function new(string name = "ded015_good_fifo");
        super.new(name);
    endfunction
endclass

class ded015_good_env extends uvm_object;
    ded015_good_fifo #(byte, 16) byte_fifo;
    ded015_good_fifo #(.DEPTH(4)) small_fifo;
    function new(string name = "ded015_good_env");
        super.new(name);
    endfunction
endclass
