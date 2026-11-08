// Clause 12: procedural statements. 12.4 if with unique/priority, 12.5 case, casez, casex,
// case inside, 12.6 pattern matching, 12.7 loops (for, repeat, foreach, while, do-while,
// forever), 12.8 break, continue, return.
module ch12_procedural;
  typedef union tagged { void None; int Some; } opt_t;
  int arr [2][3];
  logic [3:0] sel;
  opt_t o = tagged Some 7;
  initial begin
    unique if (sel == 0) $display("zero");
    else if (sel == 1) $display("one");
    priority case (sel)
      4'd0: ;
      default: ;
    endcase
    casez (sel) 4'b1??0: ; default: ; endcase
    casex (sel) 4'b1x0x: ; default: ; endcase
    case (sel) inside [0:3]: ; default: ; endcase
    case (o) matches
      tagged Some .v: $display("%0d", v);
      tagged None: ;
    endcase
    if (o matches tagged Some .w &&& w > 3) $display("big");
    for (int i = 0; i < 4; i++) begin
      if (i == 1) continue;
      if (i == 3) break;
    end
    foreach (arr[i, j]) arr[i][j] = i + j;
    repeat (2) sel++;
    while (sel > 0) sel--;
    do sel++; while (sel < 2);
    fork forever #10; join_none
  end
endmodule
