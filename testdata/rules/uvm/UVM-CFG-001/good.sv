// Compliant Clean Test Case: UVM-CFG-001 (UvmConfigDbTypeMismatch)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_uvmconfigdbtypemismatch extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// A get through a type parameter: CFG_T is whatever the specialization makes
// it, so it is not compared with the concrete type set elsewhere.
class cfg001_base_agent #(type CFG_T = uvm_object) extends uvm_component;
  CFG_T cfg;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(CFG_T)::get(this, "", "agent_cfg", cfg));
  endfunction
endclass

class cfg001_agent_cfg extends uvm_object;
  function new(string name = "cfg001_agent_cfg");
    super.new(name);
  endfunction
endclass

class cfg001_env extends uvm_component;
  cfg001_agent_cfg acfg;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    uvm_config_db#(cfg001_agent_cfg)::set(this, "agent", "agent_cfg", acfg);
  endfunction
endclass

// A field name held in a variable: two unrelated wrappers that both call
// their argument 'field_name' do not read each other's values.
class cfg001_wrappers extends uvm_component;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void put_object(string field_name, uvm_object value);
    uvm_config_db#(uvm_object)::set(this, "*", field_name, value);
  endfunction
  function bit get_string(string field_name, inout string value);
    return uvm_config_db#(string)::get(this, "", field_name, value);
  endfunction
endclass
