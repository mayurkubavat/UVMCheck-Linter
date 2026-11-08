// Compliant Clean Test Case: UVM-FAC-003 (UvmFactoryMissingUtils)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_uvmfactorymissingutils extends uvm_object;
    `uvm_object_utils(compliant_uvmfactorymissingutils)
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
