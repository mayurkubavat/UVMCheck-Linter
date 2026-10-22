#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"

namespace uvmcheck::rules::random {

class RandcMemoryVisitor : public engine::AstVisitor<RandcMemoryVisitor> {
public:
    explicit RandcMemoryVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::VariableSymbol& var) {
        if (var.getRandMode() == slang::ast::RandMode::RandC) {
            auto width = var.getType().getBitWidth();
            if (width > 16) {
                diags.addViolation(
                    "SV-RND-006",
                    "RandcMemoryExplosion",
                    "Cyclic random variable '" + std::string(var.name) + "' is declared as 'randc' with a width of " +
                    std::to_string(width) + " bits (2^" + std::to_string(width) + " states). In IEEE 1800 § 18.4.1, "
                    "randc variables require the constraint solver to maintain a non-repeating permutation table across the "
                    "entire state space. Vectors wider than 16 bits cause massive simulator heap exhaustion and extreme runtime "
                    "deadlocks during solver initialization. Restrict 'randc' to variables <= 16 bits, or use 'rand'.",
                    core::Severity::Error,
                    var.location
                );
            }
        }
        visitDefault(var);
    }

private:
    core::DiagnosticEngine& diags;
};

class RandcMemoryExplosion : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-006"; }
    std::string_view getName() const override { return "RandcMemoryExplosion"; }
    std::string_view getDescription() const override {
        return "Disallow wide vector declarations with 'randc' (> 16 bits). Cyclic permutation tracking across "
               "large state spaces causes simulator heap exhaustion and solver startup deadlocks.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        RandcMemoryVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandcMemoryExplosion() {
    return std::make_unique<random::RandcMemoryExplosion>();
}
}
