#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/CoverSymbols.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"
#include <unordered_set>
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::coverage {

class CovergroupUsageVisitor : public slang::ast::ASTVisitor<CovergroupUsageVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::AssignmentExpression& assign) {
        if (assign.right().kind == slang::ast::ExpressionKind::NewCovergroup) {
            if (assign.left().syntax) {
                std::string varName = std::string(assign.left().syntax->toString());
                varName.erase(
                    std::remove_if(varName.begin(), varName.end(),
                                   [](unsigned char c) { return std::isspace(c); }),
                    varName.end()
                );
                // Strip this. or m_ if needed, or record raw
                instantiatedCovergroups.insert(varName);
                if (varName.starts_with("this.")) {
                    instantiatedCovergroups.insert(varName.substr(5));
                }
            }
        }
        visitDefault(assign);
    }

    void handle(const slang::ast::NewCovergroupExpression& expr) {
        if (expr.type && expr.type->isCovergroup() && !expr.type->name.empty()) {
            instantiatedCovergroups.insert(std::string(expr.type->name));
        }
        visitDefault(expr);
    }

    void handle(const slang::ast::CallExpression& expr) {
        if (expr.getSubroutineName() == "sample") {
            if (expr.syntax && expr.syntax->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                std::string callee = std::string(expr.syntax->as<slang::syntax::InvocationExpressionSyntax>().left->toString());
                size_t dotPos = callee.rfind('.');
                if (dotPos != std::string::npos) {
                    std::string objName = callee.substr(0, dotPos);
                    objName.erase(
                        std::remove_if(objName.begin(), objName.end(),
                                       [](unsigned char c) { return std::isspace(c); }),
                        objName.end()
                    );
                    sampledCovergroups.insert(objName);
                    if (objName.starts_with("this.")) {
                        sampledCovergroups.insert(objName.substr(5));
                    }
                }
            }
        }
        visitDefault(expr);
    }

    std::unordered_set<std::string> instantiatedCovergroups;
    std::unordered_set<std::string> sampledCovergroups;
};

class UnreferencedCovergroup : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-001"; }
    std::string_view getName() const override { return "UnreferencedCovergroup"; }
    std::string_view getDescription() const override {
        return "Detects covergroups declared in classes or modules that are never instantiated with 'new()' "
               "or never sampled. Unreferenced covergroups contribute 0 coverage samples, creating silent verification blind spots.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        // Collect all declared covergroups
        struct DeclaredCovergroup {
            std::string name;
            std::string scopeName;
            slang::SourceLocation loc;
            bool hasEvent = false;
        };
        std::vector<DeclaredCovergroup> declaredCgs;

        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::CovergroupType) {
                    const auto& cgType = member.as<slang::ast::CovergroupType>();
                    std::string cgName = std::string(cgType.name);
                    if (cgName.empty() && cgType.getSyntax() &&
                        cgType.getSyntax()->kind == slang::syntax::SyntaxKind::CovergroupDeclaration) {
                        cgName = std::string(cgType.getSyntax()->as<slang::syntax::CovergroupDeclarationSyntax>().name.valueText());
                    }

                    if (!cgName.empty()) {
                        declaredCgs.push_back({
                            cgName,
                            std::string(cls->name),
                            cgType.location,
                            cgType.getCoverageEvent() != nullptr
                        });
                    }
                }
            }
        }

        if (declaredCgs.empty()) return;

        // Traverse AST to find instantiation and sampling
        CovergroupUsageVisitor visitor;
        compilation.getRoot().visit(visitor);

        for (const auto& cg : declaredCgs) {
            bool isInstantiated = (visitor.instantiatedCovergroups.find(cg.name) != visitor.instantiatedCovergroups.end());
            if (!isInstantiated) {
                diags.addViolation(
                    "SV-COV-001",
                    "UnreferencedCovergroup",
                    "Covergroup '" + cg.name + "' is declared in class '" + cg.scopeName +
                    "' but is never instantiated with 'new()'. Uninstantiated covergroups collect 0 coverage data, "
                    "masking unverified functional requirements.",
                    core::Severity::Warning,
                    cg.loc
                );
            } else if (!cg.hasEvent && visitor.sampledCovergroups.find(cg.name) == visitor.sampledCovergroups.end()) {
                diags.addViolation(
                    "SV-COV-001",
                    "UnreferencedCovergroup",
                    "Covergroup '" + cg.name + "' in class '" + cg.scopeName +
                    "' is instantiated but has no clocking event and is never sampled via '" + cg.name +
                    ".sample()'. It will remain at 0% coverage throughout simulation.",
                    core::Severity::Warning,
                    cg.loc
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreferencedCovergroup() {
    return std::make_unique<coverage::UnreferencedCovergroup>();
}
}
