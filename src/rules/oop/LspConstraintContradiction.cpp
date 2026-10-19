#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::oop {

class LspConstraintVisitor : public slang::syntax::SyntaxVisitor<LspConstraintVisitor> {
public:
    explicit LspConstraintVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        if (cls.extendsClause) {
            std::string className = std::string(cls.name.valueText());
            std::string baseName = std::string(cls.extendsClause->baseName->getLastToken().valueText());

            for (const auto* member : cls.items) {
                std::string memberStr = std::string(member->toString());
                if (memberStr.find("constraint_mode(0)") != std::string::npos ||
                    memberStr.find("constraint_mode(1'b0)") != std::string::npos) {
                    diags.addViolation(
                        "SV-LSP-001",
                        "LspConstraintContradiction",
                        "Derived class '" + className + "' unconditionally disables an inherited constraint via '.constraint_mode(0)'. "
                        "This violates the Liskov Substitution Principle (LSP); components receiving base class '" +
                        baseName + "' references expect base random contracts to hold. "
                        "Refactor base constraints using 'soft' constraints or constraint overriding instead of brute-force disabling.",
                        core::Severity::Error,
                        member->getFirstToken().location()
                    );
                }
            }
        }
        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class LspConstraintContradiction : public Rule {
public:
    std::string_view getId() const override { return "SV-LSP-001"; }
    std::string_view getName() const override { return "LspConstraintContradiction"; }
    std::string_view getDescription() const override {
        return "Disallows unconditionally disabling inherited constraints via '.constraint_mode(0)' in derived classes. "
               "Disabling inherited constraints breaks base class contracts and violates the Liskov Substitution Principle (LSP).";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        LspConstraintVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLspConstraintContradiction() {
    return std::make_unique<oop::LspConstraintContradiction>();
}
}
