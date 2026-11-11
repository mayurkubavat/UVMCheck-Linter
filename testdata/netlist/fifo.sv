module sync_fifo #(
    parameter int WIDTH = 8,
    parameter int DEPTH = 4
) (
    input  logic             clk,
    input  logic             rst,
    input  logic             push,
    input  logic [WIDTH-1:0] wdata,
    input  logic             pop,
    output logic [WIDTH-1:0] rdata,
    output logic             full,
    output logic             empty,
    output logic [$clog2(DEPTH):0] count
);
    localparam int AW = $clog2(DEPTH);

    logic [WIDTH-1:0] mem [0:DEPTH-1];
    logic [AW-1:0]    wptr = '0;
    logic [AW-1:0]    rptr = '0;
    logic [AW:0]      cnt  = '0;

    let do_push = push && !full;
    let do_pop  = pop  && !empty;

    assign full  = (cnt == DEPTH[AW:0]);
    assign empty = (cnt == '0);
    assign count = cnt;
    assign rdata = mem[rptr];

    always_ff @(posedge clk) begin
        if (rst) begin
            wptr <= '0;
            rptr <= '0;
            cnt  <= '0;
        end else begin
            if (do_push) begin
                mem[wptr] <= wdata;
                wptr      <= wptr + 1'b1;
            end
            if (do_pop) begin
                rptr <= rptr + 1'b1;
            end
            case ({do_push, do_pop})
                2'b10:   cnt <= cnt + 1'b1;
                2'b01:   cnt <= cnt - 1'b1;
                default: cnt <= cnt;
            endcase
        end
    end

    always_ff @(posedge clk) begin
        if (!rst) begin
            no_overflow: assert (cnt <= DEPTH[AW:0]);
        end
    end

    p_full_count: assert property (@(posedge clk) disable iff (rst) full |-> (count == DEPTH[AW:0]));
endmodule
