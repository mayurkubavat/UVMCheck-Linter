#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::correctness {

class StreamingConcatenationVisitor : public engine::SyntaxVisitor<StreamingConcatenationVisitor> {
public:
    explicit StreamingConcatenationVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::StreamingConcatenationExpressionSyntax& expr) {
        if (expr.sliceSize == nullptr) {
            std::string op = std::string(expr.operatorToken.valueText());
            diags.addViolation(
                "SV-STR-001",
                "StreamingConcatenationSizeMismatch",
                "Streaming operator '" + op + "' omits explicit slice size ('{" + op + " { ... }}'). "
                "Per IEEE 1800 § 11.4.14, omitting slice size creates an unbounded single-slice stream across operands, risking silent truncation or endianness misalignments when packing or unpacking composite types and queues. "
                "Specify an explicit slice width (e.g. '{" + op + " 8 { ... }}' or '{" + op + " byte { ... }}').",
                core::Severity::Warning,
                expr.operatorToken.location()
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class StreamingConcatenationSizeMismatch : public Rule {
public:
    std::string_view getId() const override { return "SV-STR-001"; }
    std::string_view getName() const override { return "StreamingConcatenationSizeMismatch"; }
    std::string_view getDescription() const override {
        return "Disallows streaming concatenation operators ({>> { ... }} / {<< { ... }}) without explicit slice size. "
               "Unbounded slice streaming leads to silent truncation and byte alignment bugs per IEEE 1800 § 11.4.14.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        StreamingConcatenationVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStreamingConcatenationSizeMismatch() {
    return std::make_unique<correctness::StreamingConcatenationSizeMismatch>();
}
}
