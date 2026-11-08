// UVM-API-002 (DeprecatedUvmMacro): `uvm_do_with is deprecated in UVM 2020
`include "uvm_mock.svh"

class api002_bad_seq extends uvm_sequence #(uvm_sequence_item);
    uvm_sequence_item req;
    function new(string name = "api002_bad_seq");
        super.new(name);
    endfunction
    task body();
        `uvm_do_with(req, {})
    endtask
endclass
