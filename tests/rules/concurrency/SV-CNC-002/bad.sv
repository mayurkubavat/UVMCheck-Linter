// Test for SV-CNC-002: Inverted lock acquisition order causing circular wait deadlock
module bad_deadlock;
    semaphore sem_a;
    semaphore sem_b;

    initial begin
        sem_a = new(1);
        sem_b = new(1);

        fork
            // Thread 1: acquires sem_a then sem_b
            begin
                sem_a.get(1);
                sem_b.get(1);
                sem_b.put(1);
                sem_a.put(1);
            end

            // Thread 2: VIOLATION: acquires sem_b then sem_a (deadlock cycle!)
            begin
                sem_b.get(1);
                sem_a.get(1);
                sem_a.put(1);
                sem_b.put(1);
            end
        join
    end
endmodule
