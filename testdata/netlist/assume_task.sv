module assume_task (
    input  logic clk,
    input  logic vld,
    input  logic req,
    input  logic en,
    output logic q
);
    task automatic apply_rules;
        env_req_en: assume (!req || en);
    endtask

    always_ff @(posedge clk) begin
        if (vld) begin
            apply_rules();
            q <= req & en;
        end
    end
endmodule
