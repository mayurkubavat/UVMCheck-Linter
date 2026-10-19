#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class ProgramBlockVisitor : public slang::syntax::SyntaxVisitor<ProgramBlockVisitor> {
public:
    explicit ProgramBlockVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ModuleDeclarationSyntax& decl) {
        if (decl.kind == slang::syntax::SyntaxKind::ProgramDeclaration ||
            decl.header->moduleKeyword.kind == slang::parsing::TokenKind::ProgramKeyword) {
            std::string name = decl.header->name.valueText().empty() ? "anonymous" : std::string(decl.header->name.valueText());
            std::vector<core::FixItHint> fixIts;
            if (decl.header->moduleKeyword.range().start().valid()) {
                fixIts.push_back({decl.header->moduleKeyword.range(), "module", "Replace 'program' with 'module'"});
            }
            if (decl.endmodule.range().start().valid()) {
                fixIts.push_back({decl.endmodule.range(), "endmodule", "Replace 'endprogram' with 'endmodule'"});
            }

            diags.addViolation(
                "SV-DEP-001",
                "ProgramBlockDeprecation",
                "The 'program' block '" + name +
                "' is obsolete and deprecated in modern SystemVerilog/UVM (IEEE 1800 Annex C / IEEE 1800-2017 § 24). "
                "Program blocks cause premature simulation termination ($finish when initial blocks end) and reactive scheduling hazards. "
                "Replace with 'module' and clocking blocks or standard packages.",
                core::Severity::Error,
                decl.header->moduleKeyword.location(),
                std::move(fixIts)
            );
        }
        visitDefault(decl);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProgramBlockDeprecation : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-001"; }
    std::string_view getName() const override { return "ProgramBlockDeprecation"; }
    std::string_view getDescription() const override {
        return "Disallow the obsolete SystemVerilog 'program' block. Modern verification architectures "
               "mandate 'module' with clocking blocks to prevent premature simulation termination.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        ProgramBlockVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProgramBlockDeprecation() {
    return std::make_unique<deprecated::ProgramBlockDeprecation>();
}
}
