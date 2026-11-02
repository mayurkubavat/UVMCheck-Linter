// UVM-OBJ-005 (ObjectionWithoutDescription): described objections
`include "uvm_mock.svh"

class obj005_good_test extends uvm_test;
    `uvm_component_utils(obj005_good_test)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task run_phase(uvm_phase phase);
        string why = "dynamic description";
        phase.raise_objection(this, "main traffic sequence");
        #100;
        phase.drop_objection(this, why);
    endtask
endclass

// Same method names on an unrelated class are not objections.
class obj005_counter;
    function void raise_objection(int n = 1); endfunction
    function void run(); raise_objection(); endfunction
endclass
