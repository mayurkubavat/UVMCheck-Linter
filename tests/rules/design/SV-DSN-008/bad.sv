// SV-DSN-008: mutable package variables are shared by every env and test.
package tb_globals_pkg;
  int unsigned g_err_count; // VIOLATION
  string g_test_name = "smoke"; // VIOLATION
endpackage
