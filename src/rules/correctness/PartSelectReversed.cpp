#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/diagnostics/ExpressionsDiags.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::correctness {

class PartSelectReversedVisitor : public slang::ast::ASTVisitor<PartSelectReversedVisitor, slang::ast::VisitFlags::AllGood | slang::ast::VisitFlags::Bad> {
public:
    explicit PartSelectReversedVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::SubroutineSymbol& sub) {
        sub.getBody().visit(*this);
        visitDefault(sub);
    }

    void handle(const slang::ast::ClassType& cls) {
        for (const auto& member : cls.members()) {
            if (member.kind == slang::ast::SymbolKind::Subroutine) {
                member.as<slang::ast::SubroutineSymbol>().getBody().visit(*this);
            }
        }
        visitDefault(cls);
    }

    void handle(const slang::ast::RangeSelectExpression& expr) {
        if (expr.getSelectionKind() == slang::ast::RangeSelectionKind::Simple) {
            const auto& targetVal = expr.value();
            const auto& targetType = *targetVal.type;

            if (targetType.hasFixedRange()) {
                auto declaredRange = targetType.getFixedRange();

                if (expr.left().kind == slang::ast::ExpressionKind::IntegerLiteral &&
                    expr.right().kind == slang::ast::ExpressionKind::IntegerLiteral) {

                    const auto& leftLit = expr.left().as<slang::ast::IntegerLiteral>();
                    const auto& rightLit = expr.right().as<slang::ast::IntegerLiteral>();

                    if (!leftLit.getValue().hasUnknown() && !rightLit.getValue().hasUnknown()) {
                        auto lOpt = leftLit.getValue().as<int64_t>();
                        auto rOpt = rightLit.getValue().as<int64_t>();

                        if (lOpt && rOpt) {
                            int64_t l = *lOpt;
                            int64_t r = *rOpt;

                            bool declaredDescending = declaredRange.isDescending();
                            bool sliceDescending = (l >= r);

                            if (declaredDescending != sliceDescending) {
                                std::string targetName;
                                if (targetVal.kind == slang::ast::ExpressionKind::NamedValue) {
                                    targetName = std::string(targetVal.as<slang::ast::NamedValueExpression>().symbol.name);
                                } else if (targetVal.kind == slang::ast::ExpressionKind::MemberAccess) {
                                    targetName = std::string(targetVal.as<slang::ast::MemberAccessExpression>().member.name);
                                } else if (targetVal.syntax) {
                                    targetName = std::string(targetVal.syntax->toString());
                                }
                                if (targetName.empty()) targetName = "vector";

                                std::vector<core::FixItHint> fixIts;
                                if (expr.syntax && expr.syntax->kind == slang::syntax::SyntaxKind::SimpleRangeSelect) {
                                    const auto& rngSyntax = expr.syntax->as<slang::syntax::RangeSelectSyntax>();
                                    std::string swapped = std::to_string(r) + ":" + std::to_string(l);
                                    fixIts.push_back({
                                        rngSyntax.sourceRange(),
                                        swapped,
                                        "Invert part-select bounds to [" + swapped + "]"
                                    });
                                }

                                diags.addViolation(
                                    "SV-LANG-004",
                                    "PartSelectReversed",
                                    "Inverted part-select slicing: vector '" + targetName + "' was declared as " +
                                    (declaredDescending ? "descending [" + std::to_string(declaredRange.left) + ":" + std::to_string(declaredRange.right) + "]"
                                                        : "ascending [" + std::to_string(declaredRange.left) + ":" + std::to_string(declaredRange.right) + "]") +
                                    " but sliced as [" + std::to_string(l) + ":" + std::to_string(r) + "]. "
                                    "In SystemVerilog (IEEE 1800 § 7.4.3), part-select bounds must match the declared range endianness.",
                                    core::Severity::Error,
                                    expr.sourceRange.start(),
                                    std::move(fixIts)
                                );
                            }
                        }
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class PartSelectReversed : public Rule {
public:
    std::string_view getId() const override { return "SV-LANG-004"; }
    std::string_view getName() const override { return "PartSelectReversed"; }
    std::string_view getDescription() const override {
        return "Detects reversed or inverted part-select slicing on vectors (e.g. vec[0:7] on logic [7:0] vec). "
               "Part-select slicing bounds must match the declared vector endianness.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        PartSelectReversedVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    member.as<slang::ast::SubroutineSymbol>().getBody().visit(visitor);
                }
            }
        }

        // Also check compilation diagnostics emitted by Slang for reversed range selects
        for (const auto& diag : compilation.getAllDiagnostics()) {
            if (diag.code == slang::diag::RangeSelectReversed) {
                std::vector<core::FixItHint> fixIts;
                int64_t lVal = 0, rVal = 0;
                bool hasBounds = false;
                if (diag.args.size() >= 2) {
                    const auto* lPtr = std::get_if<int64_t>(&diag.args[0]);
                    const auto* rPtr = std::get_if<int64_t>(&diag.args[1]);
                    if (lPtr && rPtr) {
                        lVal = *lPtr;
                        rVal = *rPtr;
                        hasBounds = true;
                    }
                }

                if (hasBounds && !diag.ranges.empty()) {
                    auto rng = diag.ranges[0];
                    if (rng.start().valid() && rng.end().valid()) {
                        std::string swapped = std::to_string(rVal) + ":" + std::to_string(lVal);
                        fixIts.push_back({
                            rng,
                            swapped,
                            "Invert part-select bounds to [" + swapped + "]"
                        });
                    }
                }

                std::string desc = "Inverted part-select slicing";
                if (hasBounds) {
                    desc += ": selection range [" + std::to_string(lVal) + ":" + std::to_string(rVal) + "] is reversed relative to declared vector endianness";
                }
                desc += ". In SystemVerilog (IEEE 1800 § 7.4.3), part-select bounds must match the declared range endianness.";

                diags.addViolation(
                    "SV-LANG-004",
                    "PartSelectReversed",
                    desc,
                    core::Severity::Error,
                    diag.location,
                    std::move(fixIts)
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPartSelectReversed() {
    return std::make_unique<correctness::PartSelectReversed>();
}
}
