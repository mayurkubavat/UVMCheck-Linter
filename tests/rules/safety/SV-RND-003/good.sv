// Compliant Clean Test Case: SV-RND-003 (RandomizeMissingSuper)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_randomizemissingsuper extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
