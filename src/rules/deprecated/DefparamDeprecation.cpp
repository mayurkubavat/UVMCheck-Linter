#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class DefparamVisitor : public slang::syntax::SyntaxVisitor<DefparamVisitor> {
public:
    explicit DefparamVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DefParamSyntax& defparam) {
        diags.addViolation(
            "SV-DEP-002",
            "DefparamDeprecation",
            "Hierarchical parameter override 'defparam' is formally deprecated in IEEE 1800 § 23.10.1. "
            "defparam violates module encapsulation, introduces cross-hierarchy elaboration dependencies, "
            "and prevents simulator caching. Use named parameter overrides at the instantiation site (#(.PARAM(val))).",
            core::Severity::Error,
            defparam.defparam.location()
        );
        visitDefault(defparam);
    }

private:
    core::DiagnosticEngine& diags;
};

class DefparamDeprecation : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-002"; }
    std::string_view getName() const override { return "DefparamDeprecation"; }
    std::string_view getDescription() const override {
        return "Disallow 'defparam' statements. Parameter overrides must be specified directly on instance headers.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        DefparamVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDefparamDeprecation() {
    return std::make_unique<deprecated::DefparamDeprecation>();
}
}
