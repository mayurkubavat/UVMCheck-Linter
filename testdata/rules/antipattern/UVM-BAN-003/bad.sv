// Violation Test Case: UVM-BAN-003 (DirectDoMethodCall)
`include "uvm_mock.svh"

class cfg_obj extends uvm_object;
    `uvm_object_utils(cfg_obj)
    int mode;
    function new(string name = "cfg_obj");
        super.new(name);
    endfunction : new
    virtual function void do_copy(uvm_object rhs);
        cfg_obj rhs_;
        super.do_copy(rhs);                // fine: chaining to the parent hook
        if ($cast(rhs_, rhs)) mode = rhs_.mode;
    endfunction : do_copy
endclass : cfg_obj

class cfg_user extends uvm_object;
    `uvm_object_utils(cfg_user)
    cfg_obj a, b;
    function new(string name = "cfg_user");
        super.new(name);
    endfunction : new
    function void clone_cfg();
        a.do_copy(b);                      // VIOLATION: call a.copy(b)
    endfunction : clone_cfg
endclass : cfg_user
