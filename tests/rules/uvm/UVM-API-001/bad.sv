// UVM-API-001 (AccelleraOnlyApi): print_config is Accellera-only, not IEEE 1800.2
`include "uvm_mock.svh"

class api001_bad_env extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    function void end_of_elaboration_phase(uvm_phase phase);
        print_config(1);
    endfunction
endclass
