#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/syntax/SyntaxNode.h"

#include <map>

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

namespace {

// A one-branch fork's range starts at its branch; point at the fork keyword instead.
slang::SourceLocation forkLoc(const BlockStatement& b) {
    return b.syntax ? b.syntax->sourceRange().start() : swe::locOf(b);
}

bool isFork(const Statement& s) {
    return s.kind == StatementKind::Block && s.as<BlockStatement>().blockKind != StatementBlockKind::Sequential;
}

std::vector<const Statement*> branches(const BlockStatement& fork) {
    std::vector<const Statement*> out;
    if (fork.body.kind == StatementKind::List) {
        for (const auto* s : fork.body.as<StatementList>().list) {
            if (s->kind != StatementKind::VariableDeclaration && s->kind != StatementKind::Empty) out.push_back(s);
        }
    } else {
        out.push_back(&fork.body);
    }
    return out;
}

bool usesSemaphore(const Statement& s) {
    bool found = false;
    swe::forEachCall(s, [&](const CallExpression& c) { found |= swe::isSemaphoreGet(c); });
    return found;
}

// Class properties written by a statement (container appends are excluded: they do not
// overwrite each other).
std::vector<const Symbol*> propertyWrites(const Statement& s) {
    std::vector<const Symbol*> out;
    for (const auto* sym : swe::writtenSymbols(s, false)) {
        if (sym->kind == SymbolKind::ClassProperty) out.push_back(sym);
    }
    return out;
}

class ForkWriteVisitor : public ASTVisitor<ForkWriteVisitor, VisitFlags::AllGood> {
public:
    ForkWriteVisitor(const std::string& where, core::DiagnosticEngine& diags) : where(where), diags(diags) {}

    void handle(const StatementList& list) {
        for (size_t i = 0; i < list.list.size(); i++) {
            const Statement* s = list.list[i];
            if (!isFork(*s)) continue;
            const auto& fork = s->as<BlockStatement>();
            checkBranches(fork);
            if (fork.blockKind == StatementBlockKind::JoinNone && !usesSemaphore(fork.body))
                checkParent(fork, list, i);
        }
        visitDefault(list);
    }

    void handle(const BlockStatement& block) {
        if (isFork(block) && !inList(block)) checkBranches(block);
        visitDefault(block);
    }

private:
    const std::string& where;
    core::DiagnosticEngine& diags;
    std::unordered_set<const BlockStatement*> checked;

    bool inList(const BlockStatement& b) const { return checked.count(&b) > 0; }

    void checkBranches(const BlockStatement& fork) {
        if (!checked.insert(&fork).second) return;
        auto br = branches(fork);
        if (br.size() < 2) return;
        std::map<const Symbol*, int> writers;
        for (const auto* b : br) {
            if (usesSemaphore(*b)) continue;
            for (const auto* sym : propertyWrites(*b)) writers[sym]++;
        }
        for (const auto& [sym, count] : writers) {
            if (count < 2) continue;
            report(fork, "is written by " + std::to_string(count) + " branches of the same fork", *sym);
        }
    }

    void checkParent(const BlockStatement& fork, const StatementList& list, size_t index) {
        auto mine = propertyWrites(fork.body);
        if (mine.empty()) return;
        std::unordered_set<const Symbol*> after;
        for (size_t j = index + 1; j < list.list.size(); j++) {
            if (usesSemaphore(*list.list[j])) return;
            for (const auto* sym : propertyWrites(*list.list[j])) after.insert(sym);
        }
        for (const auto* sym : mine) {
            if (after.count(sym)) report(fork, "is written by a join_none thread and by the code that forked it", *sym);
        }
    }

    void report(const BlockStatement& fork, const std::string& how, const Symbol& sym) {
        diags.addViolation(
            "SV-CNC-004", "SharedWriteFromForks",
            "Property '" + std::string(sym.name) + "' " + how + " in " + where +
                ", with no semaphore around the writes. The final value depends on thread scheduling; "
                "give each thread its own variable, or guard the writes with a semaphore.",
            core::Severity::Warning, forkLoc(fork));
    }
};

} // namespace

class SharedWriteFromForks : public Rule {
public:
    std::string_view getId() const override { return "SV-CNC-004"; }
    std::string_view getName() const override { return "SharedWriteFromForks"; }
    std::string_view getDescription() const override {
        return "Flags class properties written in two or more branches of one fork, or in a join_none "
               "thread and in its parent, without a semaphore (SpotBugs IS2_INCONSISTENT_SYNC).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* r : swe::allUserRoutines(compilation, model)) {
            std::string where = "'" + std::string(r->name) + "'";
            ForkWriteVisitor v(where, diags);
            r->getBody().visit(v);
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSharedWriteFromForks() {
    return std::make_unique<concurrency::SharedWriteFromForks>();
}
}
