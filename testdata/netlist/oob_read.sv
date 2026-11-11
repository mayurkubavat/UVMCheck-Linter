module oob_read (
    input  logic [3:0] vec,
    input  logic [2:0] idx,
    output logic       bit_dyn,
    output logic       bit_const_oob,
    output logic [3:0] mem_dyn
);
    logic [3:0] arr [0:3];
    initial begin
        arr[0] = 4'h1;
        arr[1] = 4'h2;
        arr[2] = 4'h3;
        arr[3] = 4'h4;
    end

    assign bit_dyn       = vec[idx];
    assign bit_const_oob = vec[5];
    assign mem_dyn       = arr[idx];
endmodule
