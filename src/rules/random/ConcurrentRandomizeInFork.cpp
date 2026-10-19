#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <vector>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::random {

class RandomizeTargetFinder : public slang::syntax::SyntaxVisitor<RandomizeTargetFinder> {
public:
    std::vector<std::pair<std::string, slang::SourceLocation>> targets;

    void handle(const slang::syntax::ArrayOrRandomizeMethodExpressionSyntax& expr) {
        checkMethod(expr.method);
        visitDefault(expr);
    }

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        checkMethod(inv.left);
        visitDefault(inv);
    }

    void checkMethod(const slang::syntax::ExpressionSyntax* expr) {
        if (!expr) return;
        if (expr->kind == slang::syntax::SyntaxKind::ScopedName) {
            const auto& sn = expr->as<slang::syntax::ScopedNameSyntax>();
            std::string rightName = std::string(sn.right->getLastToken().valueText());
            if (rightName == "randomize") {
                std::string targetName = std::string(sn.left->getLastToken().valueText());
                if (!targetName.empty()) {
                    targets.push_back({targetName, sn.right->getLastToken().location()});
                }
            }
        } else if (expr->kind == slang::syntax::SyntaxKind::MemberAccessExpression) {
            const auto& ma = expr->as<slang::syntax::MemberAccessExpressionSyntax>();
            if (ma.name.valueText() == "randomize" && ma.left) {
                std::string targetName = std::string(ma.left->getLastToken().valueText());
                if (!targetName.empty()) {
                    targets.push_back({targetName, ma.name.location()});
                }
            }
        } else if (expr->kind == slang::syntax::SyntaxKind::InvocationExpression) {
            checkMethod(expr->as<slang::syntax::InvocationExpressionSyntax>().left);
        }
    }
};

class ConcurrentRandomizeVisitor : public slang::syntax::SyntaxVisitor<ConcurrentRandomizeVisitor> {
public:
    explicit ConcurrentRandomizeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
            std::unordered_map<std::string, slang::SourceLocation> seenTargets;
            std::unordered_set<std::string> reported;

            for (const auto* item : block.items) {
                if (!item) continue;

                RandomizeTargetFinder finder;
                item->visit(finder);

                for (const auto& [target, loc] : finder.targets) {
                    if (seenTargets.find(target) != seenTargets.end()) {
                        if (reported.find(target) == reported.end()) {
                            reported.insert(target);
                            diags.addViolation(
                                "SV-RND-016",
                                "ConcurrentRandomizeInFork",
                                "Concurrent call to '.randomize()' on the same object handle '" + target +
                                "' across parallel fork...join threads without synchronization. Randomizing a shared object concurrently causes solver race conditions, state corruption, and non-deterministic constraint failures.",
                                core::Severity::Warning,
                                loc
                            );
                        }
                    } else {
                        seenTargets[target] = loc;
                    }
                }
            }
        }

        visitDefault(block);
    }

private:
    core::DiagnosticEngine& diags;
};

class ConcurrentRandomizeInFork : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-016"; }
    std::string_view getName() const override { return "ConcurrentRandomizeInFork"; }
    std::string_view getDescription() const override {
        return "Disallows concurrent calls to .randomize() on the same object handle across parallel fork threads without synchronization.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConcurrentRandomizeVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConcurrentRandomizeInFork() {
    return std::make_unique<random::ConcurrentRandomizeInFork>();
}
} // namespace uvmcheck::rules
