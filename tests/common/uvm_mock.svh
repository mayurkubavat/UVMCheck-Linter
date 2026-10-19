// Mock UVM classes and utility macros for UVMCheck unit testing
// Eliminates the need to install or compile standard UVM-1.2 / IEEE 1800.2 packages during lint test execution.

`ifndef UVM_MOCK_SVH
`define UVM_MOCK_SVH

`define uvm_component_utils(T) typedef T this_type;
`define uvm_object_utils(T) typedef T this_type;
`define uvm_field_int(ARG, FLAG)
`define uvm_object_utils_begin(T) `uvm_object_utils(T)
`define uvm_object_utils_end
`define uvm_do(ITEM)

// Core UVM base classes
virtual class uvm_void;
endclass

virtual class uvm_object extends uvm_void;
    string name;
    function new(string name = "");
        this.name = name;
    endfunction
    virtual function string get_name();
        return name;
    endfunction
    virtual function string get_full_name();
        return name;
    endfunction
    virtual function void print();
    endfunction
endclass

class uvm_phase extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction
    function void raise_objection(uvm_object obj = null, string desc = "", int count = 1);
    endfunction
    function void drop_objection(uvm_object obj = null, string desc = "", int count = 1);
    endfunction
endclass

virtual class uvm_component extends uvm_object;
    uvm_component m_parent;
    function new(string name, uvm_component parent);
        super.new(name);
        this.m_parent = parent;
    endfunction

    virtual function void build_phase(uvm_phase phase);
    endfunction

    virtual function void connect_phase(uvm_phase phase);
    endfunction

    virtual function void end_of_elaboration_phase(uvm_phase phase);
    endfunction

    virtual function void start_of_simulation_phase(uvm_phase phase);
    endfunction

    virtual task run_phase(uvm_phase phase);
    endtask

    virtual function void extract_phase(uvm_phase phase);
    endfunction

    virtual function void check_phase(uvm_phase phase);
    endfunction

    virtual function void report_phase(uvm_phase phase);
    endfunction

    virtual function void final_phase(uvm_phase phase);
    endfunction

    function void set_config_int(string inst_name, string field_name, int val);
    endfunction

    function void get_config_int(string field_name, ref int val);
    endfunction

    function void uvm_report_info(string id, string message, int verbosity = 300, string filename = "", int line = 0);
    endfunction

    function void uvm_report_warning(string id, string message, int verbosity = 200, string filename = "", int line = 0);
    endfunction

    function void uvm_report_error(string id, string message, int verbosity = 100, string filename = "", int line = 0);
    endfunction

    function void uvm_report_fatal(string id, string message, int verbosity = 0, string filename = "", int line = 0);
    endfunction
endclass

class uvm_printer_knobs;
    int depth;
endclass

class uvm_printer extends uvm_object;
    uvm_printer_knobs knobs;
    function new(string name = "uvm_printer");
        super.new(name);
        knobs = new();
    endfunction
endclass

class uvm_report_server;
    static function uvm_report_server get_server();
        uvm_report_server s;
        return s;
    endfunction
endclass

class uvm_test_done_objection;
    function void raise_objection(uvm_object obj = null); endfunction
    function void drop_objection(uvm_object obj = null); endfunction
endclass

virtual class uvm_sequence_item extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction
endclass

class uvm_sequencer_base extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

class uvm_sequencer#(type REQ=uvm_sequence_item, type RSP=REQ) extends uvm_sequencer_base;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

virtual class uvm_sequence_base extends uvm_sequence_item;
    uvm_sequencer_base m_sequencer;
    function new(string name = "");
        super.new(name);
    endfunction
    task start(uvm_sequencer_base sequencer = null, uvm_sequence_base parent_sequence = null, int this_priority = -1, bit call_pre_post = 1);
    endtask
    virtual task body();
    endtask
    virtual task pre_body();
    endtask
    virtual task post_body();
    endtask
endclass

virtual class uvm_sequence#(type REQ = uvm_sequence_item, type RSP = REQ) extends uvm_sequence_base;
    function new(string name = "");
        super.new(name);
    endfunction
endclass

virtual class uvm_driver#(type REQ = uvm_sequence_item, type RSP = REQ) extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

virtual class uvm_agent extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

virtual class uvm_env extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

virtual class uvm_scoreboard extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

virtual class uvm_test extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

class uvm_tlm_fifo#(type T=int) extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    function bit try_put(T t); return 1; endfunction
    function bit try_get(output T t); return 1; endfunction
    function bit try_peek(output T t); return 1; endfunction
endclass

class uvm_analysis_port#(type T=int);
    function new(string name = "", uvm_component parent = null);
    endfunction
    function void connect(uvm_analysis_port#(T) provider);
    endfunction
    function void write(T t);
    endfunction
endclass

class uvm_analysis_export#(type T=int);
    function new(string name = "", uvm_component parent = null);
    endfunction
    function void connect(uvm_analysis_export#(T) provider);
    endfunction
    function void write(T t);
    endfunction
endclass

class uvm_seq_item_pull_export#(type REQ=int, type RSP=REQ);
    function new(string name = "", uvm_component parent = null);
    endfunction
endclass

class uvm_seq_item_pull_port#(type REQ=int, type RSP=REQ);
    function new(string name = "", uvm_component parent = null);
    endfunction
    function void connect(uvm_seq_item_pull_export#(REQ, RSP) provider);
    endfunction
endclass

interface bus_if;
    logic clk;
    logic req;
    logic ack;
endinterface

class uvm_config_db #(type T = int);
    static function bit get(uvm_component cntxt, string inst_name, string field_name, ref T value);
        return 1;
    endfunction
    static function void set(uvm_component cntxt, string inst_name, string field_name, T value);
    endfunction
endclass

`ifndef MOCK_BASE_DRIVER_DEFINED
`define MOCK_BASE_DRIVER_DEFINED
// Standard test mock drivers
class BaseDriver extends uvm_component;
    `uvm_component_utils(BaseDriver)
    function new(string name = "BaseDriver", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

class MyDriver extends BaseDriver;
    `uvm_component_utils(MyDriver)
    function new(string name = "MyDriver", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass
`endif

`endif // UVM_MOCK_SVH
