// Test Case: UVM-PHS-002 (UvmTimeInFunctionPhase)
// Extracted from sample_tb.sv (Test Case 3)

`include "uvm_mock.svh"

// --- TEST CASE 3: UVM-FAC-001, UVM-PHS-001, UVM-PHS-002, SV-OOP-002, UVM-CFG-002 ---
class MyMonitor extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION UVM-PHS-001: missing super.build_phase(phase)
    // VIOLATION UVM-PHS-002: #10 delay inside function phase
    function void build_phase(uvm_phase phase);
        #10; 
    endfunction
endclass

class MyEnv extends uvm_component;
    MyDriver  driver;
    MyMonitor monitor;
    int       baud_rate;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        BaseDriver base_drv;
        super.build_phase(phase); // Compliant call

        // VIOLATION UVM-FAC-001: Direct instantiation via new instead of factory create()
        driver = new("driver", this);
        // Compliant inline waiver test: This violation is waived!
        monitor = new("monitor", this); // uvmcheck:waive UVM-FAC-001 [legacy VIP monitor]

        // VIOLATION SV-OOP-002: Unchecked $cast wrapped in void'
        void'($cast(driver, base_drv));

        // Compliant cast checking
        if (!$cast(driver, base_drv)) begin
            // safe error handling
        end

        // VIOLATION UVM-CFG-002: Passing null context to uvm_config_db inside component
        void'(uvm_config_db#(int)::get(null, "", "baud_rate", baud_rate));

        // Compliant context: passing 'this'
        void'(uvm_config_db#(int)::get(this, "", "baud_rate", baud_rate));
    endfunction
endclass
