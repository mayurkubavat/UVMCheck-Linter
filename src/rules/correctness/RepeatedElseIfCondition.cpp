#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/ConditionalStatements.h"

#include <unordered_set>

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

class RepeatedConditionVisitor : public swe::UserCodeVisitor<RepeatedConditionVisitor> {
public:
    using UserCodeVisitor::handle;

    RepeatedConditionVisitor(const Compilation& comp, core::DiagnosticEngine& diags)
        : comp(comp), diags(diags) {}

    void handle(const ConditionalStatement& cond) {
        if (!chained.count(&cond)) checkChain(cond);
        visitDefault(cond);
    }

private:
    const Compilation& comp;
    core::DiagnosticEngine& diags;
    std::unordered_set<const ConditionalStatement*> chained;

    // Conditions that can legitimately differ between evaluations.
    static bool isVolatile(const std::string& text) {
        return text.find("$urandom") != std::string::npos || text.find("$random") != std::string::npos ||
               text.find("randomize") != std::string::npos || text.find("try_") != std::string::npos ||
               text.find("++") != std::string::npos || text.find("--") != std::string::npos;
    }

    void checkChain(const ConditionalStatement& head) {
        std::vector<const Expression*> conds;
        const ConditionalStatement* curr = &head;
        while (true) {
            if (curr->conditions.size() == 1 && !curr->conditions[0].pattern)
                conds.push_back(curr->conditions[0].expr);
            if (!curr->ifFalse || curr->ifFalse->kind != StatementKind::Conditional) break;
            curr = &curr->ifFalse->as<ConditionalStatement>();
            chained.insert(curr);
        }
        for (size_t i = 1; i < conds.size(); i++) {
            if (isVolatile(swe::exprText(*conds[i]))) continue;
            for (size_t j = 0; j < i; j++) {
                if (conds[i]->isEquivalentTo(*conds[j])) {
                    diags.addViolation(
                        "SV-BUG-002", "RepeatedElseIfCondition",
                        "Condition '" + swe::exprText(*conds[i]) +
                            "' already appears at line " +
                            std::to_string(swe::lineOf(comp, swe::locOf(*conds[j]))) +
                            " in the same if/else-if chain, so this branch can never run. "
                            "It usually hides a case that was meant to be handled here.",
                        core::Severity::Warning, swe::locOf(*conds[i]));
                    break;
                }
            }
        }
    }
};

class RepeatedElseIfCondition : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-002"; }
    std::string_view getName() const override { return "RepeatedElseIfCondition"; }
    std::string_view getDescription() const override {
        return "Flags a condition that repeats an earlier condition of the same if/else-if chain, "
               "which makes the later branch dead (Sonar S1862).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        RepeatedConditionVisitor v(compilation, diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRepeatedElseIfCondition() {
    return std::make_unique<correctness::RepeatedElseIfCondition>();
}
}
