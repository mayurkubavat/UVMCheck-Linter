// SV-BUG-006 good: tolerance compare, and an exact check against zero.
class clk_checker;
  real measured_period;
  real expected_period;
  function bit ok();
    real diff = measured_period - expected_period;
    if (diff < 0) diff = -diff;
    return diff < 0.001;
  endfunction
  function bit unset();
    return measured_period == 0.0;
  endfunction
endclass
