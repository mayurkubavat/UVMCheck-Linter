#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class EmptyCovergroupVisitor : public slang::syntax::SyntaxVisitor<EmptyCovergroupVisitor> {
public:
    explicit EmptyCovergroupVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CovergroupDeclarationSyntax& node) {
        size_t coverItems = 0;
        for (const auto* m : node.members) {
            if (m->kind == slang::syntax::SyntaxKind::Coverpoint ||
                m->kind == slang::syntax::SyntaxKind::CoverCross) {
                coverItems++;
            }
        }

        if (coverItems == 0) {
            std::string cgName = node.name ? std::string(node.name.valueText()) : "unnamed_covergroup";
            diags.addViolation(
                "SV-COV-007",
                "EmptyCovergroup",
                "Covergroup '" + cgName + "' is declared with zero coverpoints or crosses. "
                "Empty covergroups collect zero verification metrics and provide a false impression of coverage closure. "
                "Define coverpoints or crosses inside the covergroup, or remove the unused declaration.",
                core::Severity::Warning,
                node.name ? node.name.location() : node.getFirstToken().location()
            );
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class EmptyCovergroup : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-007"; }
    std::string_view getName() const override { return "EmptyCovergroup"; }
    std::string_view getDescription() const override {
        return "Flags covergroups declared with zero coverpoints and zero crosses. An empty covergroup collects no coverage data.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EmptyCovergroupVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyCovergroup() {
    return std::make_unique<coverage::EmptyCovergroup>();
}
}
