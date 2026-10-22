#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::random {

class RandModeVisitor : public engine::SyntaxVisitor<RandModeVisitor> {
public:
    explicit RandModeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        checkRoutine(func.items, func.prototype->name->getLastToken().valueText());
        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;

    void checkRoutine(const slang::syntax::SyntaxList<slang::syntax::SyntaxNode>& items, std::string_view routineName) {
        slang::SourceLocation disabledLoc;
        bool hasDisable = false;
        bool hasRestore = false;
        std::string modeMethod;

        for (const auto* item : items) {
            std::string itemStr = std::string(item->toString());
            if (itemStr.find("rand_mode(0)") != std::string::npos ||
                itemStr.find("rand_mode(1'b0)") != std::string::npos) {
                hasDisable = true;
                disabledLoc = item->getFirstToken().location();
                modeMethod = "rand_mode";
            } else if (itemStr.find("rand_mode(1)") != std::string::npos ||
                       itemStr.find("rand_mode(1'b1)") != std::string::npos) {
                hasRestore = true;
            }

            if (itemStr.find("constraint_mode(0)") != std::string::npos ||
                itemStr.find("constraint_mode(1'b0)") != std::string::npos) {
                hasDisable = true;
                disabledLoc = item->getFirstToken().location();
                modeMethod = "constraint_mode";
            } else if (itemStr.find("constraint_mode(1)") != std::string::npos ||
                       itemStr.find("constraint_mode(1'b1)") != std::string::npos) {
                hasRestore = true;
            }
        }

        if (hasDisable && !hasRestore) {
            diags.addViolation(
                "SV-RND-013",
                "UnrestoredRandMode",
                "Random mode or constraint was disabled via '." + modeMethod +
                "(0)' in '" + std::string(routineName) + "()' but was never restored with '." +
                modeMethod + "(1)'. Permanently leaving random fields or constraints disabled causes silent stimulus degradation across subsequent transactions.",
                core::Severity::Warning,
                disabledLoc
            );
        }
    }
};

class UnrestoredRandMode : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-013"; }
    std::string_view getName() const override { return "UnrestoredRandMode"; }
    std::string_view getDescription() const override {
        return "Flags '.rand_mode(0)' or '.constraint_mode(0)' calls that are never restored with '1' in the routine. "
               "Unrestored disabled random modes permanently corrupt stimulus generation for subsequent transactions.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        RandModeVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnrestoredRandMode() {
    return std::make_unique<random::UnrestoredRandMode>();
}
}
