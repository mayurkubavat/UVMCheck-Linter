#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::performance {

class NbaClassPropertyVisitor : public engine::AstVisitor<NbaClassPropertyVisitor> {
public:
    explicit NbaClassPropertyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        if (assign.isNonBlocking()) {
            const auto* sym = assign.left().getSymbolReference();
            if (sym && sym->kind == slang::ast::SymbolKind::ClassProperty) {
                diags.addViolation(
                    "SV-PRF-009",
                    "NbaOnClassProperty",
                    "Non-blocking assignment ('<=') targeting class property '" + std::string(sym->name) + "'. "
                    "Unlike static module/interface registers, class instances reside on the dynamic heap. "
                    "Performing non-blocking assignments on class members forces the simulation kernel to allocate "
                    "dynamic event descriptors in the NBA event queue, causing severe scheduler queue thrashing. "
                    "Replace with blocking assignment ('=').",
                    core::Severity::Warning,
                    assign.sourceRange.start()
                );
            }
        }
        visitDefault(assign);
    }

private:
    core::DiagnosticEngine& diags;
};

class NbaOnClassProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-009"; }
    std::string_view getName() const override { return "NbaOnClassProperty"; }
    std::string_view getDescription() const override {
        return "Disallow non-blocking assignments ('<=') on class properties. For heap-allocated class instances, NBAs force the simulator kernel to create dynamic event descriptors in the NBA region of the event wheel. Use blocking assignment ('=').";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        NbaClassPropertyVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNbaOnClassProperty() {
    return std::make_unique<performance::NbaOnClassProperty>();
}
}
