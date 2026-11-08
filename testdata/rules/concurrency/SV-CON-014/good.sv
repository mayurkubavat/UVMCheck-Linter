// SV-CON-014 (DisableNamedBlockInClass): process handles and disable fork are fine
`include "uvm_mock.svh"

class con014_good_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(con014_good_driver)
    process watchdog_proc;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task run_phase(uvm_phase phase);
        fork begin
            watchdog_proc = process::self();
            #1000 $display("timeout");
        end join_none
        if (watchdog_proc != null) watchdog_proc.kill();

        fork begin
            fork
                #10;
                #20;
            join_any
            disable fork;
        end join

        for (int i = 0; i < 4; i++) begin
            if (i == 2) break;
        end
    endtask
endclass

// A named-block disable in a module is not shared between class objects.
module con014_good_top;
    initial begin : boot
        disable boot;
    end
endmodule
