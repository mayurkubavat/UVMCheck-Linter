#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/diagnostics/ExpressionsDiags.h"

namespace uvmcheck::rules::oop {

class AbstractClassVisitor : public engine::AstVisitor<AbstractClassVisitor> {
public:
    explicit AbstractClassVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::NewClassExpression& expr) {
        if (!expr.isSuperClass && expr.type->isClass()) {
            const auto& clsType = expr.type->as<slang::ast::ClassType>();
            if (clsType.isAbstract) {
                diags.addViolation(
                    "SV-OOP-013",
                    "AbstractClassInstantiation",
                    "Cannot instantiate abstract (virtual) class '" + std::string(clsType.name) + "' via 'new()'. "
                    "In SystemVerilog (IEEE 1800 § 8.21), virtual classes cannot "
                    "have direct instances; instantiate a concrete derived class instead.",
                    core::Severity::Error,
                    expr.sourceRange.start()
                );
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class AbstractClassInstantiation : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-013"; }
    std::string_view getName() const override { return "AbstractClassInstantiation"; }
    std::string_view getDescription() const override {
        return "Disallows direct instantiation of abstract (virtual) classes via 'new()'. Only concrete derived classes can be instantiated.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& compilation = ctx.compilation();
        auto& diags = ctx.diags();
        // Slang compiler emits diag::NewVirtualClass during elaboration and suppresses AST node creation.
        // Intercept both Slang's diagnostic and any created NewClassExpression nodes.
        for (const auto& diag : compilation.getAllDiagnostics()) {
            if (diag.code == slang::diag::NewVirtualClass) {
                diags.addViolation(
                    "SV-OOP-013",
                    "AbstractClassInstantiation",
                    "Cannot instantiate abstract (virtual) class via 'new()'. "
                    "In SystemVerilog (IEEE 1800 § 8.21), virtual classes cannot "
                    "have direct instances; instantiate a concrete derived class instead.",
                    core::Severity::Error,
                    diag.location
                );
            }
        }

        AbstractClassVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAbstractClassInstantiation() {
    return std::make_unique<oop::AbstractClassInstantiation>();
}
} // namespace uvmcheck::rules
