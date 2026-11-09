// Compliant Clean Test Case: SV-OOP-004 (VariableShadowing)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_variableshadowing extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// A base class's `local` member is not visible, so nothing shadows it; a
// static method sees no instance property.
class oop004_hidden_base;
    local int count;
    int scope;
endclass
class oop004_hidden_child extends oop004_hidden_base;
    function void tally();
        int count = 0;
        count++;
    endfunction
    static function int lookup(string scope);
        return scope.len();
    endfunction
endclass
