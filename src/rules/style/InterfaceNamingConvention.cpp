#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::style {

class InterfaceNamingVisitor : public engine::SyntaxVisitor<InterfaceNamingVisitor> {
public:
    explicit InterfaceNamingVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ModuleDeclarationSyntax& mod) {
        if (mod.kind == slang::syntax::SyntaxKind::InterfaceDeclaration && mod.header) {
            std::string_view name = mod.header->name.valueText();
            if (!name.empty() && !name.ends_with("_if")) {
                diags.addViolation(
                    "SV-NAM-003",
                    "InterfaceNamingConvention",
                    "Interface '" + std::string(name) + "' does not end with required suffix '_if'. "
                    "SystemVerilog methodology standards require interface identifiers to end with '_if' (e.g. '" +
                    std::string(name) + "_if') to distinguish interfaces from modules and testbench classes.",
                    core::Severity::Warning,
                    mod.header->name.location()
                );
            }
        }
        visitDefault(mod);
    }

private:
    core::DiagnosticEngine& diags;
};

class InterfaceNamingConvention : public Rule {
public:
    std::string_view getId() const override { return "SV-NAM-003"; }
    std::string_view getName() const override { return "InterfaceNamingConvention"; }
    std::string_view getDescription() const override {
        return "Enforces that interface identifiers end with '_if' (e.g., 'axi_if') to standardize interface definitions.";
    }
    std::string_view getCategory() const override { return "Naming"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        InterfaceNamingVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createInterfaceNamingConvention() {
    return std::make_unique<style::InterfaceNamingConvention>();
}
}
