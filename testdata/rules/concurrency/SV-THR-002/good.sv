// Compliant Clean Test Case: SV-THR-002 (StaticRoutineVariableLifetime)
`include "uvm_mock.svh"

package static_lifetime_ok_pkg;
    // A function returns in zero time: its calls cannot overlap, so a static
    // local is never shared between two of them.
    function int unsigned next_power_of_two(int unsigned n);
        int unsigned p;
        p = 1;
        while (p < n) p = p * 2;
        return p;
    endfunction

    // An automatic task has one copy of its locals per call.
    task automatic wait_cycles(int unsigned n);
        int unsigned left;
        left = n;
        while (left > 0) begin
            #1;
            left = left - 1;
        end
    endtask
endpackage

module static_lifetime_ok;
    // A module declared automatic makes its tasks automatic too.
endmodule

class compliant_staticroutinevariablelifetime extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction

    // Class methods are automatic.
    task hold(int unsigned n);
        int unsigned left;
        left = n;
        #(left);
    endtask
endclass
