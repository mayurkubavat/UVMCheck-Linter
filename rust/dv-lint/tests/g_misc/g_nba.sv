typedef int iq_t[$];
class item;
  int addr;
  int data[4];
endclass
class base_c;
  int m_b;
  int q[$];
endclass
class derived extends base_c;
  int m_x;
  item it;
  iq_t tq;
  function void f(item t, int qq[$]);
    int loc;
    t.addr <= 0;
    m_x <= 1;
    this.m_x <= 2;
    super.m_b <= 3;
    m_b <= 4;
    it.data[1] <= 5;
    q = q[1:$];
    this.q = this.q[1 : $];
    tq = tq[1:$];
    qq = qq[1:$];
    q = q[2:$];
  endfunction
endclass
module m;
  item h;
  int mq[$];
  initial begin
    h = new;
    h.addr <= 7;
    mq = mq[1:$];
  end
endmodule
