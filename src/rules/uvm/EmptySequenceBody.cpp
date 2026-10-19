#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class EmptySequenceBodyVisitor : public slang::syntax::SyntaxVisitor<EmptySequenceBodyVisitor> {
public:
    explicit EmptySequenceBodyVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
        : diags(diags), uvmModel(uvmModel) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& clsNode) {
        std::string prevClass = currentClass;
        bool prevIsSeq = currentIsSeq;

        currentClass = clsNode.name.valueText().empty() ? "" : std::string(clsNode.name.valueText());
        currentIsSeq = false;

        if (!currentClass.empty() && currentClass != "uvm_sequence" && currentClass != "uvm_sequence_base") {
            for (const auto* cls : uvmModel.getAllClasses()) {
                if (cls && cls->name == currentClass) {
                    if (uvmModel.isUvmSequence(*cls) || uvmModel.isDerivedFrom(*cls, "uvm_sequence_base")) {
                        currentIsSeq = true;
                    }
                    break;
                }
            }

            if (!currentIsSeq && clsNode.extendsClause && clsNode.extendsClause->baseName) {
                std::string baseName = std::string(clsNode.extendsClause->baseName->toString());
                if (baseName.find("uvm_sequence") != std::string::npos) {
                    currentIsSeq = true;
                }
            }
        }

        visitDefault(clsNode);

        currentClass = prevClass;
        currentIsSeq = prevIsSeq;
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (currentIsSeq && func.prototype && func.prototype->name) {
            std::string taskName = std::string(func.prototype->name->toString());
            taskName.erase(0, taskName.find_first_not_of(" \t\n\r"));
            if (size_t end = taskName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                taskName.erase(end + 1);
            }

            if (taskName == "body" && func.prototype->keyword.kind == slang::parsing::TokenKind::TaskKeyword) {
                bool hasMeaningfulStmt = false;
                for (const auto* item : func.items) {
                    if (!item) continue;
                    if (item->kind != slang::syntax::SyntaxKind::EmptyStatement) {
                        hasMeaningfulStmt = true;
                        break;
                    }
                }

                if (!hasMeaningfulStmt) {
                    diags.addViolation(
                        "UVM-SEQ-005",
                        "EmptySequenceBody",
                        "Sequence class '" + currentClass +
                        "' contains an empty 'body()' task. Empty sequences produce no stimulus transactions and typically indicate incomplete VIP or obsolete test scaffolding.",
                        core::Severity::Warning,
                        func.prototype->name->sourceRange().start(),
                        {}
                    );
                }
            }
        }
        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsSeq = false;
};

class EmptySequenceBody : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-005"; }
    std::string_view getName() const override { return "EmptySequenceBody"; }
    std::string_view getDescription() const override {
        return "Detects sequence classes whose body() task contains no executable statements. "
               "Empty sequence bodies generate zero stimulus transactions.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        EmptySequenceBodyVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptySequenceBody() {
    return std::make_unique<uvm::EmptySequenceBody>();
}
}
