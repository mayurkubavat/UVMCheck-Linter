// SV-CNS-006: enums with a base type, and RTL module enums, are fine.
`include "uvm_mock.svh"
package bus_pkg;
  typedef enum bit {READ, WRITE} dir_e;
  typedef enum logic [1:0] {OKAY, ERROR, RETRY} resp_e;
endpackage

module bus_fsm;
  enum {S_IDLE, S_RUN} state;
endmodule
