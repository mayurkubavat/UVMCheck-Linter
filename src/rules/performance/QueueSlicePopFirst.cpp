#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::performance {

class QueueSliceVisitor : public engine::AstVisitor<QueueSliceVisitor> {
public:
    explicit QueueSliceVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        if (assign.left().type->isQueue()) {
            const auto* lhsSym = assign.left().getSymbolReference();
            if (lhsSym) {
                const auto* rhs = &assign.right();
                while (rhs->kind == slang::ast::ExpressionKind::Conversion) {
                    rhs = &rhs->as<slang::ast::ConversionExpression>().operand();
                }

                if (rhs->kind == slang::ast::ExpressionKind::RangeSelect) {
                    const auto& rse = rhs->as<slang::ast::RangeSelectExpression>();
                    if (rse.value().getSymbolReference() == lhsSym) {
                        bool isPopFirst = false;
                        if (rse.syntax) {
                            std::string s = std::string(rse.syntax->toString());
                            if (s.find("1:$") != std::string::npos || s.find("1 : $") != std::string::npos) {
                                isPopFirst = true;
                            }
                        }

                        if (isPopFirst) {
                            std::vector<core::FixItHint> fixIts;
                            if (assign.sourceRange.start().valid()) {
                                std::string replacement = "void'(" + std::string(lhsSym->name) + ".pop_front())";
                                fixIts.push_back({assign.sourceRange, replacement, "Replace queue slice with void'(" + std::string(lhsSym->name) + ".pop_front())"});
                            }

                            diags.addViolation(
                                "SV-PRF-008",
                                "QueueSlicePopFirst",
                                "Queue slicing '" + std::string(lhsSym->name) + " = " + std::string(lhsSym->name) +
                                "[1:$]' duplicates the entire remaining queue into newly allocated memory. "
                                "Use in-place 'void'(" + std::string(lhsSym->name) + ".pop_front())' or 'item = " +
                                std::string(lhsSym->name) + ".pop_front()' for O(1) pointer adjustment.",
                                core::Severity::Warning,
                                assign.sourceRange.start(),
                                std::move(fixIts)
                            );
                        }
                    }
                }
            }
        }
        visitDefault(assign);
    }

private:
    core::DiagnosticEngine& diags;
};

class QueueSlicePopFirst : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-008"; }
    std::string_view getName() const override { return "QueueSlicePopFirst"; }
    std::string_view getDescription() const override {
        return "Disallow queue slicing ('q = q[1:$]') to drop head elements. Queue slicing duplicates all remaining elements into new memory; use pop_front() for in-place $O(1)$ head-pointer advance.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        QueueSliceVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createQueueSlicePopFirst() {
    return std::make_unique<performance::QueueSlicePopFirst>();
}
}
