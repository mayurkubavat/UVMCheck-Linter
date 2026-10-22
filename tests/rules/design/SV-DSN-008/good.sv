// SV-DSN-008 good: constants and parameters in packages are fine.
package tb_globals_pkg;
  parameter int unsigned MAX_ERRS = 10;
  localparam string DEFAULT_TEST = "smoke";
  const int unsigned BUS_WIDTH = 32;
  typedef enum {READ, WRITE} kind_e;
endpackage
