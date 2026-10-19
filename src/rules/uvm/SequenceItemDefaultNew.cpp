#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class SequenceItemDefaultNewVisitor : public slang::syntax::SyntaxVisitor<SequenceItemDefaultNewVisitor> {
public:
    explicit SequenceItemDefaultNewVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        if (!cls.extendsClause || !cls.extendsClause->baseName) {
            visitDefault(cls);
            return;
        }

        std::string baseName = std::string(cls.extendsClause->baseName->getLastToken().valueText());
        if (baseName != "uvm_sequence_item" && baseName.find("sequence_item") == std::string::npos) {
            visitDefault(cls);
            return;
        }

        std::string className = std::string(cls.name.valueText());

        for (const auto* member : cls.items) {
            if (member && member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                const auto& method = member->as<slang::syntax::ClassMethodDeclarationSyntax>();
                if (method.declaration && method.declaration->prototype && method.declaration->prototype->name) {
                    std::string methodName = std::string(method.declaration->prototype->name->getLastToken().valueText());
                    if (methodName == "new") {
                        const auto* portList = method.declaration->prototype->portList;
                        if (portList && !portList->ports.empty()) {
                            const auto* firstPort = portList->ports[0];
                            if (firstPort && firstPort->kind == slang::syntax::SyntaxKind::FunctionPort) {
                                const auto& port = firstPort->as<slang::syntax::FunctionPortSyntax>();
                                if (port.declarator && port.declarator->initializer == nullptr) {
                                    std::string portName = std::string(port.declarator->name.valueText());
                                    std::vector<core::FixItHint> fixIts;
                                    fixIts.push_back({
                                        slang::SourceRange(port.declarator->name.location() + port.declarator->name.rawText().length(),
                                                           port.declarator->name.location() + port.declarator->name.rawText().length()),
                                        " = \"" + className + "\"",
                                        "Add default constructor string argument"
                                    });

                                    diags.addViolation(
                                        "UVM-SEQ-008",
                                        "SequenceItemDefaultNew",
                                        "Constructor 'new' of sequence item class '" + className +
                                        "' defines argument '" + portName +
                                        "' without a default value. In UVM, sequence item constructors must provide a default string name (e.g. 'string name = \"" +
                                        className + "\"') to support factory creation (`uvm_object_utils) and .clone() / .copy() operations.",
                                        core::Severity::Warning,
                                        port.declarator->name.location(),
                                        fixIts
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class SequenceItemDefaultNew : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-008"; }
    std::string_view getName() const override { return "SequenceItemDefaultNew"; }
    std::string_view getDescription() const override {
        return "Requires sequence item constructor new() to provide a default value for the string name argument to support UVM factory creation and cloning.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        SequenceItemDefaultNewVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceItemDefaultNew() {
    return std::make_unique<uvm::SequenceItemDefaultNew>();
}
} // namespace uvmcheck::rules
