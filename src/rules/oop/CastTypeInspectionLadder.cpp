#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::oop {

class CastInspectionLadderVisitor : public engine::SyntaxVisitor<CastInspectionLadderVisitor> {
public:
    explicit CastInspectionLadderVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConditionalStatementSyntax& stmt) {
        // Only inspect from the head of an if-else chain
        bool isTopLevelIf = (stmt.parent == nullptr || stmt.parent->kind != slang::syntax::SyntaxKind::ElseClause);
        if (isTopLevelIf) {
            size_t castCount = 0;
            const slang::syntax::ConditionalStatementSyntax* current = &stmt;

            while (current) {
                std::string condStr = std::string(current->predicate->toString());
                if (condStr.find("$cast") != std::string::npos) {
                    castCount++;
                }

                if (current->elseClause &&
                    current->elseClause->clause->kind == slang::syntax::SyntaxKind::ConditionalStatement) {
                    current = &current->elseClause->clause->as<slang::syntax::ConditionalStatementSyntax>();
                } else {
                    current = nullptr;
                }
            }

            if (castCount >= 3) {
                diags.addViolation(
                    "SV-OCP-001",
                    "CastTypeInspectionLadder",
                    "Cascading dynamic cast ladder ($cast) detected with " + std::to_string(castCount) + " branches. "
                    "Type inspection ladders violate the Open-Closed Principle (OCP) and bypass polymorphic dispatch. "
                    "Introducing new subclasses requires modifying each ladder across the testbench. Refactor common branch operations into virtual methods on the base class.",
                    core::Severity::Warning,
                    stmt.ifKeyword.location()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class CastTypeInspectionLadder : public Rule {
public:
    std::string_view getId() const override { return "SV-OCP-001"; }
    std::string_view getName() const override { return "CastTypeInspectionLadder"; }
    std::string_view getDescription() const override {
        return "Disallows cascading $cast ladders (3+ branches) inspecting object types. "
               "Type inspection ladders violate the Open-Closed Principle (OCP); use virtual polymorphic methods instead.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CastInspectionLadderVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCastTypeInspectionLadder() {
    return std::make_unique<oop::CastTypeInspectionLadder>();
}
}
