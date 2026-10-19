#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::random {

class ConstraintComparisonVisitor : public slang::syntax::SyntaxVisitor<ConstraintComparisonVisitor> {
public:
    ConstraintComparisonVisitor(std::string_view targetName, core::DiagnosticEngine& diags)
        : targetName(targetName), diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::EqualityExpression ||
            expr.kind == slang::syntax::SyntaxKind::WildcardEqualityExpression ||
            expr.kind == slang::syntax::SyntaxKind::CaseEqualityExpression) {

            std::string leftStr = trim(std::string(expr.left->toString()));
            std::string rightStr = trim(std::string(expr.right->toString()));

            if (!leftStr.empty() && leftStr == rightStr) {
                // If neither has local:: or member access
                if (leftStr.find("local::") == std::string::npos &&
                    leftStr.find('.') == std::string::npos) {

                    std::string targetPrefix = targetName.empty() ? "item" : std::string(targetName);
                    diags.addViolation(
                        "SV-RND-004",
                        "InlineShadowNoLocal",
                        "Inline constraint '" + leftStr + " == " + rightStr + "' in 'randomize() with' shadows "
                        "the local scope variable without the 'local::' qualifier (IEEE 1800 § 18.7). Object properties "
                        "take precedence in inline constraints, causing '" + leftStr + " == " + rightStr + "' to evaluate "
                        "as a tautology on '" + targetPrefix + "." + leftStr + "' and leaving the field unconstrained. "
                        "Qualify the local variable with 'local::" + leftStr + "'.",
                        core::Severity::Error,
                        expr.sourceRange().start()
                    );
                }
            }
        }
        visitDefault(expr);
    }

private:
    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, (last - first + 1));
    }

    std::string_view targetName;
    core::DiagnosticEngine& diags;
};

class InlineShadowVisitor : public slang::syntax::SyntaxVisitor<InlineShadowVisitor> {
public:
    explicit InlineShadowVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ArrayOrRandomizeMethodExpressionSyntax& expr) {
        std::string methodStr = std::string(expr.method->toString());
        if (methodStr.find("randomize") != std::string::npos && expr.constraints) {
            std::string targetName;
            size_t dotPos = methodStr.find('.');
            if (dotPos != std::string::npos) {
                targetName = methodStr.substr(0, dotPos);
                size_t first = targetName.find_first_not_of(" \t\r\n");
                size_t last = targetName.find_last_not_of(" \t\r\n");
                if (first != std::string::npos && last != std::string::npos) {
                    targetName = targetName.substr(first, last - first + 1);
                }
            }

            ConstraintComparisonVisitor compVisitor(targetName, diags);
            expr.constraints->visit(compVisitor);
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class InlineShadowNoLocal : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-004"; }
    std::string_view getName() const override { return "InlineShadowNoLocal"; }
    std::string_view getDescription() const override {
        return "Detects inline constraint variable shadowing inside 'randomize() with' missing 'local::'. "
               "Unqualified identifiers resolve to target object properties, turning constraints into no-op tautologies.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        InlineShadowVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            syntaxTree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createInlineShadowNoLocal() {
    return std::make_unique<random::InlineShadowNoLocal>();
}
}
