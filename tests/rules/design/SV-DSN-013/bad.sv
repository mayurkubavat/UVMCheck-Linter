// SV-DSN-013 (FatalStubOverride): a subclass refusing its parent's behavior
`include "uvm_mock.svh"

class dsn013_rw_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(dsn013_rw_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    virtual task drive_write(int addr);
        #10;
    endtask
    virtual task drive_read(int addr);
        #10;
    endtask
endclass

class dsn013_ro_driver extends dsn013_rw_driver;
    `uvm_component_utils(dsn013_ro_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
    virtual task drive_write(int addr);
        `uvm_fatal("NOT_IMPL", "write channel not supported in the read-only driver")
    endtask
endclass
