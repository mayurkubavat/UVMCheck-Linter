// SV-DSN-006: template overrides that only delegate to super.
`include "uvm_mock.svh"

class bus_agent extends uvm_agent;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase); // VIOLATION
    super.build_phase(phase);
  endfunction
  function void connect_phase(uvm_phase phase); // VIOLATION
    begin
      super.connect_phase(phase);
    end
  endfunction
endclass
