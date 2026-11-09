// Compliant Clean Test Case: SV-OOP-011 (DowncastWithoutDynamicCast)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_downcastwithoutdynamiccast extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// Casts to vector and enum typedefs whose names look like UVM classes.
typedef bit [63:0] uvm_like_data_t;
typedef enum {UVM_LIKE_BIN, UVM_LIKE_HEX} uvm_like_radix_enum;
class vector_casts;
    function uvm_like_data_t widen(int v, int r);
        uvm_like_radix_enum radix = uvm_like_radix_enum'(r);
        return uvm_like_data_t'(v);
    endfunction
endclass
