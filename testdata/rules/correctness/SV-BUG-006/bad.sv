// SV-BUG-006: exact comparison of computed real values.
class clk_checker;
  real measured_period;
  real expected_period;
  function bit ok();
    return measured_period == expected_period; // VIOLATION
  endfunction
  function bit changed(real a, real b);
    return a != b; // VIOLATION
  endfunction
endclass
