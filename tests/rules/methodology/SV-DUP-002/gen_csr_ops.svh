// ---------------- AUTO-GENERATED CODE: DO NOT EDIT ----------------
// Produced by a register generator; the copies come from one template.
function automatic int csr_pack_a(int v, int s);
  int r;
  r = (v << s) | (v >> (32 - s));
  r = r ^ 32'h1234_5678 ^ (r >> 3) ^ (r << 5);
  r = r + s * 3 + (v & 32'hFF) - (v >> 24);
  return r;
endfunction

function automatic int csr_pack_b(int w, int t);
  int q;
  q = (w << t) | (w >> (32 - t));
  q = q ^ 32'h8765_4321 ^ (q >> 3) ^ (q << 5);
  q = q + t * 3 + (w & 32'hFF) - (w >> 24);
  return q;
endfunction
