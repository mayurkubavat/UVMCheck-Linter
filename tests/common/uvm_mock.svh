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

// Reporting macros, expanded the way the real UVM library expands them
`define uvm_info(ID, MSG, VERBOSITY) \
    begin \
        if (uvm_report_enabled(VERBOSITY, UVM_INFO, ID)) \
            uvm_report_info(ID, MSG, VERBOSITY, `__FILE__, `__LINE__); \
    end
`define uvm_warning(ID, MSG) \
    begin \
        if (uvm_report_enabled(UVM_NONE, UVM_WARNING, ID)) \
            uvm_report_warning(ID, MSG, UVM_NONE, `__FILE__, `__LINE__); \
    end
`define uvm_error(ID, MSG) \
    begin \
        if (uvm_report_enabled(UVM_NONE, UVM_ERROR, ID)) \
            uvm_report_error(ID, MSG, UVM_NONE, `__FILE__, `__LINE__); \
    end
`define uvm_fatal(ID, MSG) \
    begin \
        if (uvm_report_enabled(UVM_NONE, UVM_FATAL, ID)) \
            uvm_report_fatal(ID, MSG, UVM_NONE, `__FILE__, `__LINE__); \
    end

typedef enum int {UVM_NONE = 0, UVM_LOW = 100, UVM_MEDIUM = 200, UVM_HIGH = 300,
                  UVM_FULL = 400, UVM_DEBUG = 500} uvm_verbosity;
typedef enum bit [1:0] {UVM_INFO, UVM_WARNING, UVM_ERROR, UVM_FATAL} uvm_severity;
typedef int uvm_action;
parameter uvm_action UVM_NO_ACTION = 0;
parameter uvm_action UVM_DISPLAY = 1;
parameter uvm_action UVM_LOG = 2;
parameter uvm_action UVM_COUNT = 4;
parameter uvm_action UVM_EXIT = 8;

typedef class uvm_comparer;
typedef class uvm_printer;
typedef class uvm_component;

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
    virtual function void print(uvm_printer printer = null);
    endfunction
    virtual function string get_type_name();
        return "";
    endfunction
    virtual function string convert2string();
        return "";
    endfunction
    virtual function string sprint(uvm_printer printer = null);
        return "";
    endfunction
    function void copy(uvm_object rhs);
        do_copy(rhs);
    endfunction
    virtual function void do_copy(uvm_object rhs);
    endfunction
    function bit compare(uvm_object rhs, uvm_comparer comparer = null);
        return do_compare(rhs, comparer);
    endfunction
    virtual function bit do_compare(uvm_object rhs, uvm_comparer comparer);
        return 1;
    endfunction
    virtual function void do_print(uvm_printer printer);
    endfunction
    virtual function uvm_object clone();
        return null;
    endfunction

    function bit uvm_report_enabled(int verbosity, uvm_severity severity = UVM_INFO, string id = "");
        return 1;
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

class uvm_comparer;
endclass

class uvm_objection extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction
    function void set_drain_time(uvm_object obj = null, time drain = 0);
    endfunction
endclass

class uvm_phase extends uvm_object;
    uvm_objection phase_done;
    function new(string name = "");
        super.new(name);
    endfunction
    function uvm_objection get_objection();
        return phase_done;
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

    function void set_report_severity_override(uvm_severity cur_severity, uvm_severity new_severity);
    endfunction

    function void set_report_severity_id_override(uvm_severity cur_severity, string id, uvm_severity new_severity);
    endfunction

    function void set_report_severity_action(uvm_severity severity, uvm_action action);
    endfunction

    function void set_report_id_action(string id, uvm_action action);
    endfunction

    function void set_report_severity_action_hier(uvm_severity severity, uvm_action action);
    endfunction

    function uvm_component lookup(string name);
        return null;
    endfunction
endclass

class uvm_printer_knobs;
    int depth;
endclass

virtual class uvm_printer extends uvm_object;
    uvm_printer_knobs knobs;
    function new(string name = "uvm_printer");
        super.new(name);
        knobs = new();
    endfunction
endclass

class uvm_table_printer extends uvm_printer;
    function new(string name = "uvm_table_printer");
        super.new(name);
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

typedef class uvm_sequence_base;

class uvm_sequencer_base extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task lock(uvm_sequence_base sequence_ptr);
    endtask
    function void unlock(uvm_sequence_base sequence_ptr);
    endfunction
    task grab(uvm_sequence_base sequence_ptr);
    endtask
    function void ungrab(uvm_sequence_base sequence_ptr);
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
    task lock(uvm_sequencer_base sequencer = null);
    endtask
    function void unlock(uvm_sequencer_base sequencer = null);
    endfunction
    task grab(uvm_sequencer_base sequencer = null);
    endtask
    function void ungrab(uvm_sequencer_base sequencer = null);
    endfunction
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

class uvm_analysis_port#(type T=int) extends uvm_object;
    function new(string name = "", uvm_component parent = null);
        super.new(name);
    endfunction
    function void connect(uvm_object provider);
    endfunction
    function void write(T t);
    endfunction
endclass

class uvm_analysis_export#(type T=int) extends uvm_object;
    function new(string name = "", uvm_component parent = null);
        super.new(name);
    endfunction
    function void connect(uvm_object provider);
    endfunction
    function void write(T t);
    endfunction
endclass

class uvm_tlm_analysis_fifo#(type T=int) extends uvm_tlm_fifo#(T);
    uvm_analysis_export#(T) analysis_export;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
        analysis_export = new("analysis_export", this);
    endfunction
endclass

class uvm_seq_item_pull_export#(type REQ=int, type RSP=REQ) extends uvm_object;
    function new(string name = "", uvm_component parent = null);
        super.new(name);
    endfunction
endclass

class uvm_seq_item_pull_port#(type REQ=int, type RSP=REQ) extends uvm_object;
    function new(string name = "", uvm_component parent = null);
        super.new(name);
    endfunction
    function void connect(uvm_object provider);
    endfunction
endclass

class uvm_config_db #(type T = int);
    static function bit get(uvm_component cntxt, string inst_name, string field_name, ref T value);
        return 1;
    endfunction
    static function void set(uvm_component cntxt, string inst_name, string field_name, T value);
    endfunction
endclass

virtual class uvm_subscriber#(type T=int) extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    pure virtual function void write(T t);
endclass

virtual class uvm_monitor extends uvm_component;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
    endfunction
endclass

class uvm_reg extends uvm_object;
    int size_in_bytes;
    function new(string name = "", int size_bytes = 4);
        super.new(name);
        this.size_in_bytes = size_bytes;
    endfunction
    function int get_n_bytes();
        return size_in_bytes;
    endfunction
endclass

class uvm_reg_map extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction
    function void set_sequencer(uvm_sequencer_base seqr, uvm_object adapter = null);
    endfunction
    function void add_reg(uvm_reg rg, int offset, string rights = "RW");
    endfunction
endclass

class uvm_reg_block extends uvm_object;
    uvm_reg_map default_map;
    function new(string name = "");
        super.new(name);
        default_map = new("default_map");
    endfunction
endclass

class uvm_reg_adapter extends uvm_object;
    function new(string name = "");
        super.new(name);
    endfunction
endclass

class uvm_reg_predictor#(type BUSTYPE=int) extends uvm_component;
    uvm_reg_map map;
    uvm_reg_adapter adapter;
    uvm_analysis_export#(BUSTYPE) bus_in;
    function new(string name = "", uvm_component parent = null);
        super.new(name, parent);
        bus_in = new("bus_in", this);
    endfunction
endclass

class uvm_root extends uvm_component;
    function new(string name = "__top__", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    static function uvm_root get();
        return null;
    endfunction
    function uvm_component find(string comp_match);
        return null;
    endfunction
    function void find_all(string comp_match, ref uvm_component comps[$], input uvm_component comp = null);
    endfunction
endclass

uvm_root uvm_top;

typedef enum {UNKNOWN_ACTION, THROW, CAUGHT} uvm_report_catcher_action_e;

virtual class uvm_report_catcher extends uvm_object;
    typedef uvm_report_catcher_action_e action_e;
    function new(string name = "uvm_report_catcher");
        super.new(name);
    endfunction
    pure virtual function action_e catch();
    function uvm_severity get_severity(); return UVM_INFO; endfunction
    function string get_id(); return ""; endfunction
    function string get_message(); return ""; endfunction
    function void set_severity(uvm_severity severity); endfunction
    function void set_action(uvm_action action); endfunction
    function void set_message(string message); endfunction
endclass

`include "test_fixtures.svh"

`endif // UVM_MOCK_SVH
