// Test Case: SV-SYS-001 (UncheckedFopen)
// Extracted from sample_tb.sv (Test Case 110)

`include "uvm_mock.svh"

// --- TEST CASE 110: SV-SYS-001 (UncheckedFopen) ---
class UncheckedFopenBench;
    function void write_log(string msg);
        int fd;
        // VIOLATION SV-SYS-001: File descriptor not verified against 0!
        fd = $fopen("sim_output.log", "w");
        $fwrite(fd, "Log: %s\n", msg);
        $fclose(fd);
    endfunction
endclass
