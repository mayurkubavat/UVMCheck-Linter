#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::uvm {

class SequenceBodyWithoutStartItemVisitor : public slang::syntax::SyntaxVisitor<SequenceBodyWithoutStartItemVisitor> {
public:
    explicit SequenceBodyWithoutStartItemVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
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
                inspectBodyStatements(func);
            }
        }
        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsSeq = false;

    void inspectBodyStatements(const slang::syntax::FunctionDeclarationSyntax& func) {
        std::unordered_set<std::string> startedItems;

        for (const auto* item : func.items) {
            if (!item) continue;
            checkStatement(*item, startedItems);
        }
    }

    void checkStatement(const slang::syntax::SyntaxNode& node, std::unordered_set<std::string>& startedItems) {
        if (node.kind == slang::syntax::SyntaxKind::ExpressionStatement) {
            const auto& stmt = node.as<slang::syntax::ExpressionStatementSyntax>();
            if (stmt.expr && stmt.expr->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                const auto& inv = stmt.expr->as<slang::syntax::InvocationExpressionSyntax>();
                std::string methodName;
                if (inv.left) {
                    if (inv.left->kind == slang::syntax::SyntaxKind::IdentifierName) {
                        methodName = std::string(inv.left->as<slang::syntax::IdentifierNameSyntax>().identifier.valueText());
                    } else if (inv.left->kind == slang::syntax::SyntaxKind::MemberAccessExpression) {
                        methodName = std::string(inv.left->as<slang::syntax::MemberAccessExpressionSyntax>().name.valueText());
                    }
                }

                std::string argName = "req";
                if (inv.arguments && !inv.arguments->parameters.empty() && inv.arguments->parameters[0]) {
                    argName = std::string(inv.arguments->parameters[0]->toString());
                    argName.erase(0, argName.find_first_not_of(" \t\n\r"));
                    if (size_t end = argName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        argName.erase(end + 1);
                    }
                }

                if (methodName == "start_item") {
                    startedItems.insert(argName);
                } else if (methodName == "finish_item") {
                    if (startedItems.find(argName) == startedItems.end()) {
                        diags.addViolation(
                            "UVM-SEQ-009",
                            "SequenceBodyWithoutStartItem",
                            "Call to 'finish_item(" + argName + ")' in sequence '" + currentClass +
                            "' body() is not preceded by a matching 'start_item(" + argName + ")'. "
                            "In UVM sequence arbitration, start_item() requests sequencer grant and locks driver synchronization "
                            "before finish_item() sends the transaction. Calling finish_item() without start_item() causes fatal sequencer errors.",
                            core::Severity::Error,
                            inv.sourceRange().start()
                        );
                    }
                }
            }
        } else if (node.kind == slang::syntax::SyntaxKind::SequentialBlockStatement) {
            const auto& block = node.as<slang::syntax::BlockStatementSyntax>();
            for (const auto* stmt : block.items) {
                if (stmt) checkStatement(*stmt, startedItems);
            }
        }
    }
};

class SequenceBodyWithoutStartItem : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-009"; }
    std::string_view getName() const override { return "SequenceBodyWithoutStartItem"; }
    std::string_view getDescription() const override {
        return "Flags calls to finish_item() inside uvm_sequence::body() that are not preceded by a corresponding start_item(). "
               "The UVM sequencer arbitration handshake strictly requires start_item() before finish_item().";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        SequenceBodyWithoutStartItemVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceBodyWithoutStartItem() {
    return std::make_unique<uvm::SequenceBodyWithoutStartItem>();
}
}
