#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::oop {

class ForeachShadowVisitor : public slang::syntax::SyntaxVisitor<ForeachShadowVisitor> {
public:
    explicit ForeachShadowVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        auto prevProps = currentClassProps;
        currentClassProps.clear();

        // Collect class properties
        for (const auto* member : cls.items) {
            const slang::syntax::DataDeclarationSyntax* dataDecl = nullptr;
            if (member->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                dataDecl = &member->as<slang::syntax::DataDeclarationSyntax>();
            } else if (member->kind == slang::syntax::SyntaxKind::ClassPropertyDeclaration) {
                const auto& prop = member->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                if (prop.declaration && prop.declaration->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                    dataDecl = &prop.declaration->as<slang::syntax::DataDeclarationSyntax>();
                }
            }

            if (dataDecl) {
                for (const auto* decl : dataDecl->declarators) {
                    currentClassProps.insert(std::string(decl->name.valueText()));
                }
            }
        }

        visitDefault(cls);
        currentClassProps = prevProps;
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        auto prevLocals = currentLocalVars;
        currentLocalVars.clear();

        // Collect parameters / arguments
        if (func.prototype && func.prototype->portList) {
            for (const auto* port : func.prototype->portList->ports) {
                if (port->kind == slang::syntax::SyntaxKind::FunctionPort) {
                    const auto& fp = port->as<slang::syntax::FunctionPortSyntax>();
                    if (fp.declarator) {
                        currentLocalVars.insert(std::string(fp.declarator->name.valueText()));
                    }
                }
            }
        }

        // Collect local variable declarations
        for (const auto* item : func.items) {
            if (item->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                const auto& dd = item->as<slang::syntax::DataDeclarationSyntax>();
                for (const auto* decl : dd.declarators) {
                    currentLocalVars.insert(std::string(decl->name.valueText()));
                }
            }
        }

        visitDefault(func);
        currentLocalVars = prevLocals;
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& loop) {
        if (loop.loopList) {
            for (const auto* loopVar : loop.loopList->loopVariables) {
                if (!loopVar) continue;
                std::string varName = std::string(loopVar->getLastToken().valueText());
                if (varName.empty()) continue;

                if (currentClassProps.count(varName) > 0) {
                    diags.addViolation(
                        "SV-SHD-002",
                        "ForeachLoopVariableShadowing",
                        "Foreach loop index variable '" + varName + "' shadows class property '" + varName +
                        "'. Per IEEE 1800 § 12.7.3, foreach loop variables are implicitly declared in the local loop scope and shadow outer identifiers with the same name, masking class state. Rename the loop index (e.g. '" +
                        varName + "_idx').",
                        core::Severity::Warning,
                        loopVar->getFirstToken().location()
                    );
                } else if (currentLocalVars.count(varName) > 0) {
                    diags.addViolation(
                        "SV-SHD-002",
                        "ForeachLoopVariableShadowing",
                        "Foreach loop index variable '" + varName + "' shadows local variable '" + varName +
                        "' declared in the enclosing routine. Per IEEE 1800 § 12.7.3, foreach loop variables create a nested scope, masking routine variables. Rename the loop index (e.g. '" +
                        varName + "_idx').",
                        core::Severity::Warning,
                        loopVar->getFirstToken().location()
                    );
                }
            }
        }

        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
    std::unordered_set<std::string> currentClassProps;
    std::unordered_set<std::string> currentLocalVars;
};

class ForeachLoopVariableShadowing : public Rule {
public:
    std::string_view getId() const override { return "SV-SHD-002"; }
    std::string_view getName() const override { return "ForeachLoopVariableShadowing"; }
    std::string_view getDescription() const override {
        return "Flags loop index variables in 'foreach (arr[i])' loops that shadow an existing class property or routine local variable.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ForeachShadowVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createForeachLoopVariableShadowing() {
    return std::make_unique<oop::ForeachLoopVariableShadowing>();
}
}
