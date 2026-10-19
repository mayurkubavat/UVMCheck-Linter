// Test Case: SV-CON-008 (MutexTryGetWithoutCheck)
// Extracted from sample_tb.sv (Test Case 178)

`include "uvm_mock.svh"

// --- TEST CASE 178: SV-CON-008 (MutexTryGetWithoutCheck) ---
class MutexTryGetBench;
    semaphore sem_lock;
    task critical_section();
        sem_lock = new(1);
        // VIOLATION SV-CON-008: Calling try_get() without checking return value risks entering critical section without lock!
        sem_lock.try_get(1);
    endtask
endclass
