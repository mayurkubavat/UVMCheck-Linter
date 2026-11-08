// Declarations used from b.sv, which is compiled as a separate file.
package mf_pkg;
  typedef enum logic [1:0] { IDLE, BUSY, DONE } state_e;

  virtual class shape;
    pure virtual function int area();
  endclass
endpackage
