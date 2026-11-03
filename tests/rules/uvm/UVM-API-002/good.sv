// UVM-API-002 (DeprecatedUvmMacro): start_item/finish_item instead of deprecated macros
`include "uvm_mock.svh"

class api002_good_seq extends uvm_sequence #(uvm_sequence_item);
    uvm_sequence_item req;
    function new(string name = "api002_good_seq");
        super.new(name);
    endfunction
    task body();
        `uvm_do(req)
    endtask
endclass
