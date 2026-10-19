#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include <queue>

namespace uvmcheck::rules::random {

class CyclicSolveBeforeVisitor : public slang::syntax::SyntaxVisitor<CyclicSolveBeforeVisitor> {
public:
    explicit CyclicSolveBeforeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& node) {
        // Collect solve-before constraints within this class scope
        std::unordered_map<std::string, std::vector<std::string>> adj;

        class SolveBeforeCollector : public slang::syntax::SyntaxVisitor<SolveBeforeCollector> {
        public:
            std::vector<const slang::syntax::SolveBeforeConstraintSyntax*> solveNodes;
            void handle(const slang::syntax::SolveBeforeConstraintSyntax& sbc) {
                solveNodes.push_back(&sbc);
            }
        };

        SolveBeforeCollector collector;
        node.visit(collector);

        auto hasPath = [&](const std::string& start, const std::string& target) -> bool {
            if (start == target) return true;
            std::queue<std::string> q;
            std::unordered_set<std::string> visited;
            q.push(start);
            visited.insert(start);
            while (!q.empty()) {
                std::string curr = q.front();
                q.pop();
                auto it = adj.find(curr);
                if (it != adj.end()) {
                    for (const auto& next : it->second) {
                        if (next == target) return true;
                        if (!visited.count(next)) {
                            visited.insert(next);
                            q.push(next);
                        }
                    }
                }
            }
            return false;
        };

        auto cleanName = [](std::string s) -> std::string {
            s.erase(0, s.find_first_not_of(" \t\n\r"));
            if (size_t end = s.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                s.erase(end + 1);
            }
            return s;
        };

        for (const auto* sbc : collector.solveNodes) {
            std::vector<std::string> beforeVars;
            std::vector<std::string> afterVars;
            for (const auto* b : sbc->beforeExpr) {
                if (b) beforeVars.push_back(cleanName(b->toString()));
            }
            for (const auto* a : sbc->afterExpr) {
                if (a) afterVars.push_back(cleanName(a->toString()));
            }

            for (const auto& u : beforeVars) {
                for (const auto& v : afterVars) {
                    if (u.empty() || v.empty()) continue;
                    if (u == v || hasPath(v, u)) {
                        diags.addViolation(
                            "SV-RND-010",
                            "CyclicSolveBefore",
                            "Cyclic dependency in solve...before order involving variables '" + u +
                            "' and '" + v + "'. Cyclic solve-before constraints form an unsatisfiable dependency graph per IEEE 1800 § 18.5.10.",
                            core::Severity::Error,
                            sbc->sourceRange().start(),
                            {}
                        );
                    } else {
                        adj[u].push_back(v);
                    }
                }
            }
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class CyclicSolveBefore : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-010"; }
    std::string_view getName() const override { return "CyclicSolveBefore"; }
    std::string_view getDescription() const override {
        return "Detects cyclic dependencies in solve...before constraint ordering (e.g. solve a before b and solve b before a). "
               "Cyclic solve-before constraints create an unsatisfiable dependency graph in the solver.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        CyclicSolveBeforeVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCyclicSolveBefore() {
    return std::make_unique<random::CyclicSolveBefore>();
}
}
