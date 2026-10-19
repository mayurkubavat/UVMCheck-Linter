// Compliant Clean Test Case: SV-RND-018 (RandomizeInConstructor)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_randomizeinconstructor extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass
