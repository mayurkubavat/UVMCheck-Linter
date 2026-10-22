#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"

namespace uvmcheck::rules::random {

class RandcStateVisitor : public engine::AstVisitor<RandcStateVisitor> {
public:
    explicit RandcStateVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::VariableSymbol& var) {
        if (var.getRandMode() == slang::ast::RandMode::RandC) {
            auto width = var.getType().getBitWidth();
            if (width > 0 && width <= 2) {
                uint64_t states = 1ULL << width;
                diags.addViolation(
                    "SV-RND-017",
                    "RandcStateExhaustion",
                    "Cyclic random variable '" + std::string(var.name) + "' is declared as 'randc' with a width of " +
                    std::to_string(width) + " bit" + (width > 1 ? "s" : "") + " (only " + std::to_string(states) +
                    " states). In IEEE 1800 § 18.4.1, 'randc' forces the constraint solver to maintain a non-repeating "
                    "permutation table. For variables <= 2 bits, cyclic permutation history resets every " +
                    std::to_string(states) + " iterations, incurring solver tracking overhead with no statistical "
                    "permutation benefit. Use standard 'rand' instead.",
                    core::Severity::Warning,
                    var.location
                );
            }
        }
        visitDefault(var);
    }

private:
    core::DiagnosticEngine& diags;
};

class RandcStateExhaustion : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-017"; }
    std::string_view getName() const override { return "RandcStateExhaustion"; }
    std::string_view getDescription() const override {
        return "Flags 'randc' cyclic variables declared with bit-width <= 2 bits (1 or 2 bits). "
               "State permutation tables for <= 4 states reset almost immediately, incurring solver overhead with no benefit.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        RandcStateVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandcStateExhaustion() {
    return std::make_unique<random::RandcStateExhaustion>();
}
}
