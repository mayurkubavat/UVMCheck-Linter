typedef int my_t;
typedef class fwd;
class fwd; endclass
wire w1;
logic l1, l2;
import "DPI-C" cname = function void c_f(input reg r);
class k;
  int arr[4];
  my_t q[$];
  task f();
    foreach (arr[i]) arr[i] = 1;
    foreach (this.arr[i]) q.push_back(i);
    arr[0] = -1;

    #0 arr[1] = 1;
    #(0);
    #0ns;
  endtask
endclass
