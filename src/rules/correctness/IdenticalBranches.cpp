#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/MiscStatements.h"

#include <unordered_set>

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

// A branch is worth comparing when it does real work: lone return/break/continue
// statements and report-only branches are commonly repeated on purpose.
static bool isTrivialBranch(const Statement& stmt) {
    const Statement* s = &stmt;
    while (s->kind == StatementKind::Block) s = &s->as<BlockStatement>().body;
    switch (s->kind) {
        case StatementKind::Empty:
        case StatementKind::Return:
        case StatementKind::Break:
        case StatementKind::Continue:
            return true;
        case StatementKind::List:
            return s->as<StatementList>().list.empty();
        default:
            break;
    }
    if (swe::countStatements(*s) <= 2 && swe::strongestReport(*s) != swe::ReportLevel::None) {
        return true; // a single `uvm_* macro (if + call)
    }
    return false;
}

// "`uvm_info(...)" and similar: one report macro as the whole branch.
static bool isSingleMacroCall(const std::string& text) {
    if (!text.starts_with("`uvm_")) return false;
    auto semi = text.find(';');
    return semi == std::string::npos || semi + 1 == text.size();
}

class IdenticalBranchesVisitor : public swe::UserCodeVisitor<IdenticalBranchesVisitor> {
public:
    using UserCodeVisitor::handle;

    IdenticalBranchesVisitor(const Compilation& comp, core::DiagnosticEngine& diags)
        : comp(comp), diags(diags) {}

    void handle(const ConditionalStatement& cond) {
        if (chained.count(&cond)) {
            visitDefault(cond);
            return;
        }
        std::vector<const Statement*> branches;
        const ConditionalStatement* curr = &cond;
        while (true) {
            branches.push_back(&curr->ifTrue);
            if (!curr->ifFalse) break;
            if (curr->ifFalse->kind == StatementKind::Conditional) {
                curr = &curr->ifFalse->as<ConditionalStatement>();
                chained.insert(curr);
                continue;
            }
            branches.push_back(curr->ifFalse);
            break;
        }
        compare(branches, "if/else");
        visitDefault(cond);
    }

    void handle(const CaseStatement& cs) {
        std::vector<const Statement*> branches;
        for (const auto& item : cs.items) branches.push_back(item.stmt);
        if (cs.defaultCase) branches.push_back(cs.defaultCase);
        compare(branches, "case");
        visitDefault(cs);
    }

private:
    const Compilation& comp;
    core::DiagnosticEngine& diags;
    std::unordered_set<const ConditionalStatement*> chained;

    void compare(const std::vector<const Statement*>& branches, const char* what) {
        std::vector<std::string> texts;
        texts.reserve(branches.size());
        for (const auto* b : branches) texts.push_back(swe::originalText(comp, *b));
        for (size_t i = 1; i < branches.size(); i++) {
            if (texts[i].empty() || isTrivialBranch(*branches[i]) || isSingleMacroCall(texts[i])) continue;
            for (size_t j = 0; j < i; j++) {
                if (texts[i] == texts[j]) {
                    diags.addViolation(
                        "SV-BUG-001", "IdenticalBranches",
                        std::string("This ") + what + " branch is identical to the branch at line " +
                            std::to_string(swe::lineOf(comp, swe::locOf(*branches[j]))) +
                            ". A pasted branch that was never edited is the usual cause; if both "
                            "really do the same thing, merge the conditions.",
                        core::Severity::Warning, swe::locOf(*branches[i]));
                    break;
                }
            }
        }
    }
};

class IdenticalBranches : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-001"; }
    std::string_view getName() const override { return "IdenticalBranches"; }
    std::string_view getDescription() const override {
        return "Flags if/else and case branches whose bodies are token-identical to an earlier branch "
               "of the same statement (clang-tidy bugprone-branch-clone, Sonar S1871).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        IdenticalBranchesVisitor v(compilation, diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIdenticalBranches() {
    return std::make_unique<correctness::IdenticalBranches>();
}
}
