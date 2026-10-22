// SV-DSN-002: one class does prediction, coverage and config handling with unrelated methods.
class ref_cfg;
  int f0;
  int f1;
  int f2;
  int f3;
  int f4;
  int f5;
  int f6;
endclass
class ref_model;
  int f0;
  int f1;
  int f2;
  int f3;
  int f4;
  int f5;
  int f6;
endclass
class cov_state;
  int f0;
  int f1;
  int f2;
  int f3;
  int f4;
  int f5;
  int f6;
endclass

class soc_scoreboard; // VIOLATION
  int m0;
  int m1;
  int m2;
  int m3;
  int m4;
  int m5;
  int m6;
  int m7;
  int m8;
  int m9;
  int m10;
  int m11;

  function void job0(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 0) m0 = cfg.f0;
    else if (v < 0) m0 = mdl.f0;
    else m0 = 0;
    if (cov.f0 && v == 1) m0++;
  endfunction
  function void job1(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 1) m1 = cfg.f1;
    else if (v < 0) m1 = mdl.f1;
    else m1 = 0;
    if (cov.f1 && v == 1) m1++;
  endfunction
  function void job2(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 2) m2 = cfg.f2;
    else if (v < 0) m2 = mdl.f2;
    else m2 = 0;
    if (cov.f2 && v == 1) m2++;
  endfunction
  function void job3(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 3) m3 = cfg.f3;
    else if (v < 0) m3 = mdl.f3;
    else m3 = 0;
    if (cov.f3 && v == 1) m3++;
  endfunction
  function void job4(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 4) m4 = cfg.f4;
    else if (v < 0) m4 = mdl.f4;
    else m4 = 0;
    if (cov.f4 && v == 1) m4++;
  endfunction
  function void job5(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 5) m5 = cfg.f5;
    else if (v < 0) m5 = mdl.f5;
    else m5 = 0;
    if (cov.f5 && v == 1) m5++;
  endfunction
  function void job6(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 6) m6 = cfg.f6;
    else if (v < 0) m6 = mdl.f6;
    else m6 = 0;
    if (cov.f6 && v == 1) m6++;
  endfunction
  function void job7(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 7) m7 = cfg.f0;
    else if (v < 0) m7 = mdl.f0;
    else m7 = 0;
    if (cov.f0 && v == 1) m7++;
  endfunction
  function void job8(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 8) m8 = cfg.f1;
    else if (v < 0) m8 = mdl.f1;
    else m8 = 0;
    if (cov.f1 && v == 1) m8++;
  endfunction
  function void job9(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 9) m9 = cfg.f2;
    else if (v < 0) m9 = mdl.f2;
    else m9 = 0;
    if (cov.f2 && v == 1) m9++;
  endfunction
  function void job10(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 10) m10 = cfg.f3;
    else if (v < 0) m10 = mdl.f3;
    else m10 = 0;
    if (cov.f3 && v == 1) m10++;
  endfunction
  function void job11(ref_cfg cfg, ref_model mdl, cov_state cov, int v);
    if (v > 11) m11 = cfg.f4;
    else if (v < 0) m11 = mdl.f4;
    else m11 = 0;
    if (cov.f4 && v == 1) m11++;
  endfunction
endclass
