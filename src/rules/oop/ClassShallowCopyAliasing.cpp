#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::oop {

static bool hasNestedClassHandle(const slang::ast::ClassType& classType) {
    const auto* curr = &classType;
    while (curr) {
        for (const auto& member : curr->members()) {
            if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                if (prop.getType().getCanonicalType().isClass()) {
                    return true;
                }
            }
        }
        const auto* base = curr->getBaseClass();
        curr = (base && base->isClass()) ? &base->as<slang::ast::ClassType>() : nullptr;
    }
    return false;
}

class ShallowCopyVisitor : public engine::AstVisitor<ShallowCopyVisitor> {
public:
    explicit ShallowCopyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::CopyClassExpression& expr) {
        if (expr.type->isClass()) {
            const auto& classType = expr.type->as<slang::ast::ClassType>();
            if (hasNestedClassHandle(classType)) {
                diags.addViolation(
                    "SV-OOP-006",
                    "ClassShallowCopyAliasing",
                    "Shallow copy 'new " + std::string(expr.sourceExpr().syntax ? expr.sourceExpr().syntax->toString() : "other") +
                    "' on class '" + std::string(classType.name) + "' duplicates primitive fields but copies nested class handles by reference. "
                    "Both instances will share the same nested object handles in memory, creating data corruption and race conditions. "
                    "Implement a deep copy method (e.g. clone() or copy()) instead.",
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

class ClassShallowCopyAliasing : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-006"; }
    std::string_view getName() const override { return "ClassShallowCopyAliasing"; }
    std::string_view getDescription() const override {
        return "Using shallow copy (new other_obj) on classes containing nested object handles creates shared "
               "references across independent transactions. Use deep copy or clone() instead.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ShallowCopyVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createClassShallowCopyAliasing() {
    return std::make_unique<oop::ClassShallowCopyAliasing>();
}
}
