#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::style {

class PackageNamingVisitor : public slang::syntax::SyntaxVisitor<PackageNamingVisitor> {
public:
    explicit PackageNamingVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ModuleDeclarationSyntax& mod) {
        if (mod.kind == slang::syntax::SyntaxKind::PackageDeclaration && mod.header) {
            std::string_view name = mod.header->name.valueText();
            if (!name.empty() && !name.ends_with("_pkg")) {
                diags.addViolation(
                    "SV-NAM-002",
                    "PackageNamingConvention",
                    "Package '" + std::string(name) + "' does not end with required suffix '_pkg'. "
                    "SystemVerilog methodology standards require package identifiers to end with '_pkg' (e.g. '" +
                    std::string(name) + "_pkg') to clarify namespace scopes and prevent collision with modules.",
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

class PackageNamingConvention : public Rule {
public:
    std::string_view getId() const override { return "SV-NAM-002"; }
    std::string_view getName() const override { return "PackageNamingConvention"; }
    std::string_view getDescription() const override {
        return "Enforces that package identifiers end with '_pkg' (e.g., 'axi_pkg') to standardize namespaces.";
    }
    std::string_view getCategory() const override { return "Naming"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        PackageNamingVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPackageNamingConvention() {
    return std::make_unique<style::PackageNamingConvention>();
}
}
