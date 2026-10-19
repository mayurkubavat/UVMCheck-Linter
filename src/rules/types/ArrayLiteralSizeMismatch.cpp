#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/diagnostics/ExpressionsDiags.h"

namespace uvmcheck::rules::types {

class ArrayLiteralSizeVisitor : public slang::ast::ASTVisitor<ArrayLiteralSizeVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ArrayLiteralSizeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::SimpleAssignmentPatternExpression& pattern) {
        if (pattern.type && pattern.type->getCanonicalType().kind == slang::ast::SymbolKind::FixedSizeUnpackedArrayType) {
            const auto& arrType = pattern.type->getCanonicalType().as<slang::ast::FixedSizeUnpackedArrayType>();
            size_t expected = arrType.range.width();
            size_t actual = pattern.elements().size();

            if (actual != expected && actual > 0) {
                diags.addViolation(
                    "SV-TYP-009",
                    "ArrayLiteralSizeMismatch",
                    "Array literal assignment pattern provides " + std::to_string(actual) +
                    " element(s), but target fixed-size array has dimension of " + std::to_string(expected) +
                    " element(s). In IEEE 1800 § 10.9 array assignment semantics, "
                    "positional assignment patterns must provide an element count matching the target array dimension.",
                    core::Severity::Warning,
                    pattern.sourceRange.start()
                );
            }
        }
        visitDefault(pattern);
    }

private:
    core::DiagnosticEngine& diags;
};

class ArrayLiteralSizeMismatch : public Rule {
public:
    std::string_view getId() const override { return "SV-TYP-009"; }
    std::string_view getName() const override { return "ArrayLiteralSizeMismatch"; }
    std::string_view getDescription() const override {
        return "Flags positional array assignment patterns '{...} where the number of elements does not match the target fixed array dimension.";
    }
    std::string_view getCategory() const override { return "Types"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        // Intercept both Slang's diagnostic for pattern count mismatch and ASTVisitor
        for (const auto& diag : compilation.getAllDiagnostics()) {
            if (diag.code == slang::diag::WrongNumberAssignmentPatterns) {
                diags.addViolation(
                    "SV-TYP-009",
                    "ArrayLiteralSizeMismatch",
                    "Array literal assignment pattern element count does not match target array dimension. "
                    "In IEEE 1800 § 10.9 array assignment semantics, "
                    "positional assignment patterns must provide an element count matching the target array dimension.",
                    core::Severity::Warning,
                    diag.location
                );
            }
        }

        ArrayLiteralSizeVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::types

namespace uvmcheck::rules {
std::unique_ptr<Rule> createArrayLiteralSizeMismatch() {
    return std::make_unique<types::ArrayLiteralSizeMismatch>();
}
} // namespace uvmcheck::rules
