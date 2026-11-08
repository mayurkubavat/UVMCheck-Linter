// SV-DSN-013 (FatalStubOverride): overrides that do work, and pure methods
`include "uvm_mock.svh"

virtual class dsn013_good_base extends uvm_object;
    function new(string name = "dsn013_good_base");
        super.new(name);
    endfunction
    pure virtual function void check_it(int v);
    virtual function void report_it(int v);
        $display("%0d", v);
    endfunction
endclass

class dsn013_good_impl extends dsn013_good_base;
    function new(string name = "dsn013_good_impl");
        super.new(name);
    endfunction
    virtual function void check_it(int v);
        `uvm_error("CHK", "not expected")   // implements a pure method: nothing refused
    endfunction
    virtual function void report_it(int v);
        if (v < 0) `uvm_error("NEG", "negative value")
        else super.report_it(v);
    endfunction
endclass
