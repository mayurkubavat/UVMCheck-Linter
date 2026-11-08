class holder;
  local function int hidden();
    return 1;
  endfunction : hidden
  function int shown();
    return hidden();
  endfunction : shown
endclass : holder
