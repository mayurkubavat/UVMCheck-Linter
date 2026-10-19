#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <algorithm>

namespace uvmcheck::rules::random {

class RedundantConstraintVisitor : public slang::syntax::SyntaxVisitor<RedundantConstraintVisitor> {
public:
    explicit RedundantConstraintVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& node) {
        std::string className = node.name.valueText().empty() ? "unnamed" : std::string(node.name.valueText());

        // Collect all expression constraints in this class
        class ExprConstraintCollector : public slang::syntax::SyntaxVisitor<ExprConstraintCollector> {
        public:
            std::vector<const slang::syntax::ExpressionConstraintSyntax*> items;
            void handle(const slang::syntax::ExpressionConstraintSyntax& item) {
                items.push_back(&item);
            }
        };

        ExprConstraintCollector collector;
        node.visit(collector);

        std::unordered_map<std::string, const slang::syntax::ExpressionConstraintSyntax*> seen;

        auto stripCommentsAndWhitespace = [](const std::string& input) -> std::string {
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
        };

        for (const auto* item : collector.items) {
            if (!item || !item->expr) continue;
            std::string rawExpr = std::string(item->expr->toString());
            // Trim whitespace
            rawExpr.erase(0, rawExpr.find_first_not_of(" \t\n\r"));
            if (size_t end = rawExpr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                rawExpr.erase(end + 1);
            }
            std::string cleanExpr = rawExpr;
            if (cleanExpr.rfind('\n') != std::string::npos) {
                cleanExpr = cleanExpr.substr(cleanExpr.rfind('\n') + 1);
                cleanExpr.erase(0, cleanExpr.find_first_not_of(" \t\n\r"));
            }
            std::string key = (!item->soft.isMissing() ? "soft:" : "") + stripCommentsAndWhitespace(rawExpr);

            auto it = seen.find(key);
            if (it != seen.end()) {
                std::vector<core::FixItHint> fixIts;
                if (item->sourceRange().start().valid()) {
                    fixIts.push_back({
                        item->sourceRange(),
                        "",
                        "Remove duplicate constraint expression '" + cleanExpr + "'"
                    });
                }

                diags.addViolation(
                    "SV-RND-011",
                    "RedundantConstraint",
                    "Duplicate constraint expression '" + cleanExpr + "' in class '" + className +
                    "'. Duplicate constraints bloat the solver constraint graph and increase solver compile overhead without introducing new assertions.",
                    core::Severity::Warning,
                    item->sourceRange().start(),
                    std::move(fixIts)
                );
            } else {
                seen[key] = item;
            }
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class RedundantConstraint : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-011"; }
    std::string_view getName() const override { return "RedundantConstraint"; }
    std::string_view getDescription() const override {
        return "Detects duplicate or redundant constraint expressions within the same class scope. "
               "Redundant constraints enlarge solver expression graphs and increase solver solve latency without adding constraints.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        RedundantConstraintVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRedundantConstraint() {
    return std::make_unique<random::RedundantConstraint>();
}
}
