#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"

namespace uvmcheck::rules::deprecated {

class RandomSystemFunctionVisitor : public slang::ast::ASTVisitor<RandomSystemFunctionVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit RandomSystemFunctionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.getSubroutineName() == "$random") {
            diags.addViolation(
                "SV-DEP-004",
                "RandomSystemFunction",
                "Verilog system function '$random' is superseded in IEEE 1800 § 20.15. "
                "'$random' generates signed 32-bit integers that evaluate to negative values when used with modulo, "
                "and shares a single global LCG seed that corrupts simulation random stability. "
                "Replace with '$urandom' or '$urandom_range(min, max)'.",
                core::Severity::Warning,
                call.sourceRange.start()
            );
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
};

class RandomSystemFunction : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-004"; }
    std::string_view getName() const override { return "RandomSystemFunction"; }
    std::string_view getDescription() const override {
        return "Disallow legacy Verilog '$random'. Replaced by '$urandom' and '$urandom_range' in IEEE 1800.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        RandomSystemFunctionVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomSystemFunction() {
    return std::make_unique<deprecated::RandomSystemFunction>();
}
}
