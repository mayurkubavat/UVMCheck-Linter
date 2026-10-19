// Compliant Clean Test Case: UVM-CFG-002 (UvmConfigDbNullContext)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_uvmconfigdbnullcontext extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
