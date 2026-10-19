#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class ProceduralAssignVisitor : public slang::syntax::SyntaxVisitor<ProceduralAssignVisitor> {
public:
    explicit ProceduralAssignVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ProceduralAssignStatementSyntax& stmt) {
        std::string kw = std::string(stmt.keyword.valueText());
        diags.addViolation(
            "SV-DEP-005",
            "ProceduralContinuousAssign",
            "Procedural continuous assignment ('" + kw +
            "') is non-synthesizable and strongly discouraged in IEEE 1800 § 10.6. "
            "Procedural assign/deassign statements create simulator-synthesis mismatches, break formal verification, "
            "and induce unpredictable dynamic driver overriding races. Use standard blocking/non-blocking procedural assignments.",
            core::Severity::Error,
            stmt.keyword.location()
        );
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProceduralContinuousAssign : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-005"; }
    std::string_view getName() const override { return "ProceduralContinuousAssign"; }
    std::string_view getDescription() const override {
        return "Disallow procedural 'assign' and 'deassign' statements. Procedural continuous assignments "
               "are non-synthesizable and induce simulation race conditions.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        ProceduralAssignVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProceduralContinuousAssign() {
    return std::make_unique<deprecated::ProceduralContinuousAssign>();
}
}
