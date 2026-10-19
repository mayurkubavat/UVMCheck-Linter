#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <algorithm>
#include <cctype>
#include <unordered_set>

namespace uvmcheck::rules::performance {

class AssocDeclCollector : public slang::syntax::SyntaxVisitor<AssocDeclCollector> {
public:
    std::unordered_set<std::string> assocArrays;

    void handle(const slang::syntax::DeclaratorSyntax& decl) {
        if (!decl.dimensions.empty()) {
            for (const auto* dim : decl.dimensions) {
                if (!dim) continue;
                std::string dimStr = std::string(dim->toString());
                // Trim whitespace
                dimStr.erase(0, dimStr.find_first_not_of(" \t\n\r"));
                if (size_t end = dimStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                    dimStr.erase(end + 1);
                }

                // Associative arrays have [type] or [*] (no ':' and no '$')
                if (dimStr.size() >= 3 && dimStr.front() == '[' && dimStr.back() == ']' &&
                    dimStr.find(':') == std::string::npos && dimStr.find('$') == std::string::npos) {
                    // Check if content inside is a type or wildcard or identifier
                    std::string inside = dimStr.substr(1, dimStr.size() - 2);
                    inside.erase(0, inside.find_first_not_of(" \t\n\r"));
                    if (size_t end = inside.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        inside.erase(end + 1);
                    }
                    if (!inside.empty()) {
                        std::string varName = std::string(decl.name.valueText());
                        if (!varName.empty()) {
                            assocArrays.insert(varName);
                        }
                    }
                }
            }
        }
        visitDefault(decl);
    }
};

class AssocReadVisitor : public slang::syntax::SyntaxVisitor<AssocReadVisitor> {
public:
    AssocReadVisitor(const std::unordered_set<std::string>& arrays, core::DiagnosticEngine& diags)
        : assocArrays(arrays), diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        switch (expr.kind) {
            case slang::syntax::SyntaxKind::EqualityExpression:
            case slang::syntax::SyntaxKind::InequalityExpression:
            case slang::syntax::SyntaxKind::CaseEqualityExpression:
            case slang::syntax::SyntaxKind::CaseInequalityExpression:
            case slang::syntax::SyntaxKind::LessThanExpression:
            case slang::syntax::SyntaxKind::LessThanEqualExpression:
            case slang::syntax::SyntaxKind::GreaterThanExpression:
            case slang::syntax::SyntaxKind::GreaterThanEqualExpression:
                checkSelect(expr.left);
                checkSelect(expr.right);
                break;
            default:
                break;
        }
        visitDefault(expr);
    }

private:
    void checkSelect(const slang::syntax::ExpressionSyntax* expr) {
        if (!expr) return;
        std::string varName;
        if (const auto* idSelect = expr->as_if<slang::syntax::IdentifierSelectNameSyntax>()) {
            if (!idSelect->selectors.empty() && idSelect->identifier.valid()) {
                varName = std::string(idSelect->identifier.valueText());
            }
        } else if (const auto* select = expr->as_if<slang::syntax::ElementSelectExpressionSyntax>()) {
            if (select->left) {
                varName = std::string(select->left->getLastToken().valueText());
            }
        }

        if (!varName.empty() && assocArrays.count(varName)) {
            diags.addViolation(
                "SV-ASC-001",
                "AssociativeArrayMutatingRead",
                "Direct read indexing on associative array '" + varName +
                "' in relational comparison. In SystemVerilog (IEEE 1800 § 7.8.1), reading a non-existent key silently inserts the key and populates it with a default value, mutating array state and leaking memory. Guard the lookup with '" +
                varName + ".exists(...)' before indexing.",
                core::Severity::Warning,
                expr->sourceRange().start()
            );
        }
    }

    const std::unordered_set<std::string>& assocArrays;
    core::DiagnosticEngine& diags;
};

class AssociativeArrayMutatingRead : public Rule {
public:
    std::string_view getId() const override { return "SV-ASC-001"; }
    std::string_view getName() const override { return "AssociativeArrayMutatingRead"; }
    std::string_view getDescription() const override {
        return "Detects direct read lookups on associative arrays in expressions without checking exists(). "
               "In SystemVerilog (IEEE 1800 § 7.8.1), reading a missing key silently allocates and inserts it with a default value.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        AssocDeclCollector collector;
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) tree->root().visit(collector);
        }

        AssocReadVisitor visitor(collector.assocArrays, diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAssociativeArrayMutatingRead() {
    return std::make_unique<performance::AssociativeArrayMutatingRead>();
}
}
