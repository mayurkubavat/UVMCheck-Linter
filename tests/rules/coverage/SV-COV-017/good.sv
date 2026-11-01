// Compliant Test Case: SV-COV-017 (UnreachableCoverBin)
`include "uvm_mock.svh"

class len_cov;
    bit [4:0] len;
    covergroup cg_len;
        cp_len: coverpoint len {
            bins short_len = {[0:7]};
            bins long_len  = {[8:15]};
            bins huge_len  = {[16:31]};
        }
    endgroup
    function new();
        cg_len = new();
    endfunction : new
endclass : len_cov
