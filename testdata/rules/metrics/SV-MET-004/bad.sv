// SV-MET-004: six levels of nesting.
class deep_model;
  int q[$];
  function void scan(int n);
    for (int i = 0; i < n; i++) begin
      if (i > 0) begin
        foreach (q[j]) begin
          while (q[j] > 0) begin
            if (q[j] % 2 == 0) begin
              repeat (2) q[j]--; // VIOLATION
            end
            q[j]--;
          end
        end
      end
    end
  endfunction
endclass
