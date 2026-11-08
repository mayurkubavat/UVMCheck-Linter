// UVM-API-001 (AccelleraOnlyApi): IEEE 1800.2 methods only
`include "uvm_mock.svh"

class api001_good_env extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void end_of_elaboration_phase(uvm_phase phase);
        super.end_of_elaboration_phase(phase);
    endfunction
endclass
