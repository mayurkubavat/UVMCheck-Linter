// SV-CON-014 (DisableNamedBlockInClass): named-block and task disables in class methods
`include "uvm_mock.svh"

class con014_driver extends uvm_driver #(uvm_sequence_item);
    `uvm_component_utils(con014_driver)
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    task watch();
        #100;
    endtask

    task run_phase(uvm_phase phase);
        fork : watch_dog
            #1000 $display("timeout");
        join_none
        disable watch_dog;

        begin : scan
            for (int i = 0; i < 4; i++) begin
                if (i == 2) disable scan;
            end
        end

        fork watch(); join_none
        disable watch;
    endtask
endclass
