#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class CaseWithoutDefaultVisitor : public engine::SyntaxVisitor<CaseWithoutDefaultVisitor> {
public:
    explicit CaseWithoutDefaultVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CaseStatementSyntax& stmt) {
        bool hasDefault = false;
        for (const auto* item : stmt.items) {
            if (item->kind == slang::syntax::SyntaxKind::DefaultCaseItem) {
                hasDefault = true;
                break;
            }
        }

        if (!hasDefault) {
            std::string keywordStr = std::string(stmt.caseKeyword.valueText());
            diags.addViolation(
                "SV-COR-006",
                "CaseWithoutDefault",
                "Statement '" + keywordStr +
                "' lacks an explicit 'default:' fallback branch. In verification state decoders, bus monitors, and packet parsers, omitting a default branch causes unhandled transaction edge cases and inadvertent simulation latch behavior.",
                core::Severity::Warning,
                stmt.caseKeyword.location()
            );
        }

        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class CaseWithoutDefault : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-006"; }
    std::string_view getName() const override { return "CaseWithoutDefault"; }
    std::string_view getDescription() const override {
        return "Flags 'case', 'casex', and 'casez' statements that lack an explicit 'default:' fallback branch.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CaseWithoutDefaultVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCaseWithoutDefault() {
    return std::make_unique<correctness::CaseWithoutDefault>();
}
}
