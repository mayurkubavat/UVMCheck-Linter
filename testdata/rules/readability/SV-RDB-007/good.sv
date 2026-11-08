// SV-RDB-007 (NonDescriptiveName): descriptive names; short helpers and loop counters are fine
`include "uvm_mock.svh"

class rdb007_good_scb;
    int unsigned expected_count;
    bit [31:0] aligned_addr;
    function int twice(int value);   // argument: not checked
        int tmp;                      // short routine: not checked
        tmp = value * 2;
        return tmp;
    endfunction
    function void sweep();
        for (int i = 0; i < 4; i++) begin
            expected_count += i;
            expected_count += 1;
            expected_count += 2;
            expected_count += 3;
            expected_count += 4;
            expected_count += 5;
            expected_count += 6;
            expected_count += 7;
        end
    endfunction
endclass
