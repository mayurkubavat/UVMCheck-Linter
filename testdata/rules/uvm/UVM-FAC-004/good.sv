// Compliant Clean Test Case: UVM-FAC-004 (ParameterizedClassFactoryRegistration)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_parameterizedclassfactoryregistration extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
