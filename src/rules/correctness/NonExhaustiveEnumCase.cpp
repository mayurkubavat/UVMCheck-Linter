#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::correctness {

class NonExhaustiveEnumCaseVisitor : public slang::ast::ASTVisitor<NonExhaustiveEnumCaseVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit NonExhaustiveEnumCaseVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CaseStatement& caseStmt) {
        const auto& exprType = *caseStmt.expr.type;
        if (exprType.isEnum()) {
            const auto& enumType = exprType.getCanonicalType().as<slang::ast::EnumType>();

            std::unordered_set<std::string> declaredVariants;
            for (const auto& member : enumType.values()) {
                declaredVariants.insert(std::string(member.name));
            }

            std::unordered_set<std::string> handledVariants;
            for (const auto& item : caseStmt.items) {
                for (const auto* expr : item.expressions) {
                    while (expr->kind == slang::ast::ExpressionKind::Conversion) {
                        expr = &expr->as<slang::ast::ConversionExpression>().operand();
                    }
                    if (expr->kind == slang::ast::ExpressionKind::NamedValue) {
                        handledVariants.insert(std::string(expr->as<slang::ast::NamedValueExpression>().symbol.name));
                    } else if (const auto* cv = expr->getConstant()) {
                        for (const auto& member : enumType.values()) {
                            if (member.getValue() == *cv) {
                                handledVariants.insert(std::string(member.name));
                            }
                        }
                    }
                }
            }

            if (caseStmt.defaultCase != nullptr) {
                std::vector<std::string> unhandled;
                for (const auto& var : declaredVariants) {
                    if (handledVariants.count(var) == 0) {
                        unhandled.push_back(var);
                    }
                }

                if (!unhandled.empty()) {
                    std::string missingList;
                    for (size_t i = 0; i < unhandled.size(); ++i) {
                        if (i > 0) missingList += ", ";
                        missingList += "'" + unhandled[i] + "'";
                    }

                    diags.addViolation(
                        "SV-FSM-001",
                        "NonExhaustiveEnumCase",
                        "Case statement switching over enum '" + std::string(enumType.name) +
                        "' does not explicitly handle variant(s): " + missingList + ". "
                        "Using a catch-all 'default:' arm silently masks unhandled states and allows specification drift bugs to pass undetected. "
                        "Explicitly enumerate all variants or remove 'default:' to enforce full pattern exhaustiveness.",
                        core::Severity::Warning,
                        caseStmt.sourceRange.start()
                    );
                }
            }
        }

        visitDefault(caseStmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class NonExhaustiveEnumCase : public Rule {
public:
    std::string_view getId() const override { return "SV-FSM-001"; }
    std::string_view getName() const override { return "NonExhaustiveEnumCase"; }
    std::string_view getDescription() const override {
        return "Case statement switching over an enum type does not explicitly handle all declared enum variants while using a default: arm. "
               "Catch-all default arms silently swallow newly added specification states and mask FSM decoder bugs.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }
    uint32_t getProfiles() const override {
        return static_cast<uint32_t>(RuleProfile::Soundness);
    }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        NonExhaustiveEnumCaseVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonExhaustiveEnumCase() {
    return std::make_unique<correctness::NonExhaustiveEnumCase>();
}
}
