// Test Case: SV-CON-005 (SemaphoreZeroKey)
// Extracted from sample_tb.sv (Test Case 152)

`include "uvm_mock.svh"

// --- TEST CASE 152: SV-CON-005 (SemaphoreZeroKey) ---
class SemaphoreKeyBench;
    semaphore sem_key;
    task test_sem();
        sem_key = new(1);
        // VIOLATION SV-CON-005: sem.get(0) causes simulator deadlock!
        sem_key.get(0);
    endtask
endclass
