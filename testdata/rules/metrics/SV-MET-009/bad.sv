// SV-MET-009 (ConfigKnobBudget): one flat config with too many knobs
`include "uvm_mock.svh"

class met009_env_cfg extends uvm_object;
    bit has_axi;
    bit has_apb;
    bit has_spi;
    bit has_i2c;
    bit en_scoreboard;
    bit en_coverage;
    int unsigned axi_timeout;
    int unsigned apb_timeout;
    int unsigned spi_clk_div;
    int unsigned i2c_addr;
    int unsigned n_masters;
    int unsigned n_slaves;
    int unsigned max_outstanding;
    function new(string name = "met009_env_cfg");
        super.new(name);
    endfunction
endclass
