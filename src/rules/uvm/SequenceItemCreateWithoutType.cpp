#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class SeqItemCreateVisitor : public slang::syntax::SyntaxVisitor<SeqItemCreateVisitor> {
public:
    explicit SeqItemCreateVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
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
                inspectBody(func);
            }
        }
        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsSeq = false;

    void inspectBody(const slang::syntax::FunctionDeclarationSyntax& func) {
        for (const auto* item : func.items) {
            if (!item) continue;
            checkNode(*item);
        }
    }

    void checkNode(const slang::syntax::SyntaxNode& node) {
        if (node.kind == slang::syntax::SyntaxKind::ExpressionStatement) {
            const auto& stmt = node.as<slang::syntax::ExpressionStatementSyntax>();
            if (stmt.expr && stmt.expr->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                const auto& bin = stmt.expr->as<slang::syntax::BinaryExpressionSyntax>();
                if (bin.right && bin.right->kind == slang::syntax::SyntaxKind::NewClassExpression) {
                    std::string lhs = std::string(bin.left->toString());
                    lhs.erase(0, lhs.find_first_not_of(" \t\n\r"));
                    if (size_t end = lhs.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        lhs.erase(end + 1);
                    }

                    if (lhs == "req" || lhs == "rsp" || lhs.find("item") != std::string::npos ||
                        lhs.find("tr") != std::string::npos || lhs.find("pkt") != std::string::npos) {
                        diags.addViolation(
                            "UVM-SEQ-010",
                            "SequenceItemCreateWithoutType",
                            "Sequence item '" + lhs + "' in sequence '" + currentClass +
                            "' body() is instantiated via direct 'new()'. "
                            "In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory "
                            "(e.g. '" + lhs + " = my_item::type_id::create(\"" + lhs + "\")' or 'create_item(...)') "
                            "to enable factory type overrides for error injection and stimulus extension.",
                            core::Severity::Warning,
                            bin.right->sourceRange().start()
                        );
                    }
                }
            }
        } else if (node.kind == slang::syntax::SyntaxKind::DataDeclaration) {
            const auto& decl = node.as<slang::syntax::DataDeclarationSyntax>();
            for (const auto* d : decl.declarators) {
                if (d && d->initializer && d->initializer->expr &&
                    d->initializer->expr->kind == slang::syntax::SyntaxKind::NewClassExpression) {
                    std::string varName = std::string(d->name.valueText());
                    if (varName == "req" || varName == "rsp" || varName.find("item") != std::string::npos ||
                        varName.find("tr") != std::string::npos || varName.find("pkt") != std::string::npos) {
                        diags.addViolation(
                            "UVM-SEQ-010",
                            "SequenceItemCreateWithoutType",
                            "Sequence item '" + varName + "' in sequence '" + currentClass +
                            "' body() is initialized via direct 'new()'. "
                            "In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory "
                            "(e.g. 'type_id::create()') to enable factory overrides.",
                            core::Severity::Warning,
                            d->initializer->expr->sourceRange().start()
                        );
                    }
                }
            }
        }

        for (uint32_t i = 0; i < node.getChildCount(); i++) {
            const auto* child = node.childNode(i);
            if (child) {
                checkNode(*child);
            }
        }
    }
};

class SequenceItemCreateWithoutType : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-010"; }
    std::string_view getName() const override { return "SequenceItemCreateWithoutType"; }
    std::string_view getDescription() const override {
        return "Flags sequence items created via direct new() inside sequence body(). "
               "Sequence items must be created via factory type_id::create() to allow testcase overrides.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        SeqItemCreateVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceItemCreateWithoutType() {
    return std::make_unique<uvm::SequenceItemCreateWithoutType>();
}
}
