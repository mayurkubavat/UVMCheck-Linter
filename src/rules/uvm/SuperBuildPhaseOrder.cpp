#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class SuperBuildPhaseOrderVisitor : public slang::syntax::SyntaxVisitor<SuperBuildPhaseOrderVisitor> {
public:
    explicit SuperBuildPhaseOrderVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
        : diags(diags), uvmModel(uvmModel) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& clsNode) {
        std::string prevClass = currentClass;
        bool prevIsComp = currentIsComp;

        currentClass = clsNode.name.valueText().empty() ? "" : std::string(clsNode.name.valueText());
        currentIsComp = false;

        if (!currentClass.empty() && currentClass != "uvm_component") {
            for (const auto* cls : uvmModel.getAllClasses()) {
                if (cls && cls->name == currentClass) {
                    if (uvmModel.isUvmComponent(*cls)) {
                        currentIsComp = true;
                    }
                    break;
                }
            }

            if (!currentIsComp && clsNode.extendsClause && clsNode.extendsClause->baseName) {
                std::string baseName = std::string(clsNode.extendsClause->baseName->toString());
                if (baseName.find("uvm_component") != std::string::npos ||
                    baseName.find("uvm_env") != std::string::npos ||
                    baseName.find("uvm_agent") != std::string::npos ||
                    baseName.find("uvm_driver") != std::string::npos ||
                    baseName.find("uvm_monitor") != std::string::npos ||
                    baseName.find("uvm_scoreboard") != std::string::npos ||
                    baseName.find("uvm_test") != std::string::npos) {
                    currentIsComp = true;
                }
            }
        }

        visitDefault(clsNode);

        currentClass = prevClass;
        currentIsComp = prevIsComp;
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (currentIsComp && func.prototype && func.prototype->name) {
            std::string_view funcName = func.prototype->name->getLastToken().valueText();
            if (funcName == "build_phase") {
                checkBuildPhaseBody(func);
            }
        }
        visitDefault(func);
    }

private:
    static std::string stripCommentsAndWhitespace(const std::string& input) {
        std::string out;
        bool inLineComment = false;
        bool inBlockComment = false;
        for (size_t i = 0; i < input.size(); ++i) {
            if (inLineComment) {
                if (input[i] == '\n') inLineComment = false;
                continue;
            }
            if (inBlockComment) {
                if (input[i] == '*' && i + 1 < input.size() && input[i + 1] == '/') {
                    inBlockComment = false;
                    i++;
                }
                continue;
            }
            if (input[i] == '/' && i + 1 < input.size() && input[i + 1] == '/') {
                inLineComment = true;
                i++;
                continue;
            }
            if (input[i] == '/' && i + 1 < input.size() && input[i + 1] == '*') {
                inBlockComment = true;
                i++;
                continue;
            }
            if (input[i] != ' ' && input[i] != '\t' && input[i] != '\n' && input[i] != '\r') {
                out += input[i];
            }
        }
        return out;
    }

    void checkBuildPhaseBody(const slang::syntax::FunctionDeclarationSyntax& func) {
        bool seenProceduralStmt = false;

        for (const auto* item : func.items) {
            if (!item) continue;
            // Ignore variable/data declarations and empty statements
            if (item->kind == slang::syntax::SyntaxKind::DataDeclaration ||
                item->kind == slang::syntax::SyntaxKind::EmptyStatement) {
                continue;
            }

            std::string cleanText = stripCommentsAndWhitespace(std::string(item->toString()));
            bool isSuperBuild = (cleanText.find("super.build_phase") != std::string::npos);

            if (isSuperBuild) {
                if (seenProceduralStmt) {
                    diags.addViolation(
                        "UVM-PHS-005",
                        "SuperBuildPhaseOrder",
                        "'super.build_phase(phase)' in class '" + currentClass +
                        "' is called after preceding procedural statements. In UVM, build_phase executes top-down; "
                        "calling super.build_phase(phase) as the first statement ensures base-class configurations and sub-components are initialized before derived configuration.",
                        core::Severity::Warning,
                        item->sourceRange().start(),
                        {}
                    );
                }
                break;
            } else {
                seenProceduralStmt = true;
            }
        }
    }

    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsComp = false;
};

class SuperBuildPhaseOrder : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-005"; }
    std::string_view getName() const override { return "SuperBuildPhaseOrder"; }
    std::string_view getDescription() const override {
        return "Enforces that super.build_phase(phase) is the very first procedural statement in build_phase. "
               "Top-down phasing requires base class build_phase to execute before derived class configuration logic.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        SuperBuildPhaseOrderVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSuperBuildPhaseOrder() {
    return std::make_unique<uvm::SuperBuildPhaseOrder>();
}
}
