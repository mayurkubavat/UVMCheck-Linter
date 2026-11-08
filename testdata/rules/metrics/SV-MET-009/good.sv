// SV-MET-009 (ConfigKnobBudget): hierarchical configs stay within budget
`include "uvm_mock.svh"

class met009_axi_cfg extends uvm_object;
    int unsigned timeout;
    int unsigned max_outstanding;
    function new(string name = "met009_axi_cfg");
        super.new(name);
    endfunction
endclass

class met009_good_env_cfg extends uvm_object;
    met009_axi_cfg axi_cfg;
    bit en_scoreboard;
    bit en_coverage;
    static int unsigned instances;      // not a knob
    const int unsigned VERSION = 1;     // not a knob
    function new(string name = "met009_good_env_cfg");
        super.new(name);
    endfunction
endclass

// Not a config object: the budget does not apply.
class met009_big_item extends uvm_object;
    int a, b, c, d, e, f, g, h, i, j, k, l, m, n;
    function new(string name = "met009_big_item");
        super.new(name);
    endfunction
endclass
