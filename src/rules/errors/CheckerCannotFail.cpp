#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

#include <cctype>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::errors {

using namespace slang::ast;

namespace {

std::string lower(std::string_view s) {
    std::string out;
    for (char c : s) out += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return out;
}

bool looksLikeChecker(const ClassType& cls) {
    swe::Role role = swe::classRole(cls);
    if (role == swe::Role::Scoreboard) return true;
    // Names only count for components that receive transactions, not for objects.
    if (role != swe::Role::Component && role != swe::Role::Subscriber) return false;
    std::string n = lower(cls.name);
    return n.find("scoreboard") != std::string::npos || n.find("checker") != std::string::npos ||
           n.ends_with("_scb") || n.find("_scb_") != std::string::npos || n.ends_with("_sb");
}

struct FailFinder : public ASTVisitor<FailFinder, VisitFlags::AllGood> {
    bool canFail = false;
    std::vector<const SubroutineSymbol*> callees;
    void handle(const ImmediateAssertionStatement&) { canFail = true; }
    void handle(const ConcurrentAssertionStatement&) { canFail = true; }
    void handle(const CallExpression& call) {
        if (swe::reportLevel(call) >= swe::ReportLevel::Error) canFail = true;
        if (!call.isSystemCall()) {
            if (const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine)) {
                if (*sub) callees.push_back(*sub);
            }
        }
        visitDefault(call);
    }
};

class Analyzer {
public:
    bool classCanFail(const ClassType& cls) {
        const ClassType* c = &cls;
        int guard = 0;
        while (c && guard++ < 32) {
            std::string_view n = c->genericClass ? c->genericClass->name : c->name;
            // The library comparators report mismatches themselves.
            if (n.find("comparator") != std::string_view::npos) return true;
            if (!swe::isLibrarySymbol(*c)) {
                for (const auto* m : swe::classMethods(*c)) {
                    if (routineCanFail(*m, 0)) return true;
                }
                for (const auto& member : c->members()) {
                    if (member.kind != SymbolKind::ClassProperty) continue;
                    const auto* pc = swe::asClass(member.as<ClassPropertySymbol>().getType());
                    if (!pc) continue;
                    std::string pn = lower(pc->genericClass ? pc->genericClass->name : pc->name);
                    if (pn.find("comparator") != std::string::npos) return true;
                }
            }
            const Type* base = c->getBaseClass();
            c = base ? swe::asClass(*base) : nullptr;
        }
        return false;
    }

private:
    std::unordered_map<const SubroutineSymbol*, bool> memo;

    bool routineCanFail(const SubroutineSymbol& sub, int depth) {
        if (auto it = memo.find(&sub); it != memo.end()) return it->second;
        memo[&sub] = false; // break recursion
        if (swe::isLibrarySymbol(sub) || sub.flags.has(MethodFlags::BuiltIn)) return false;
        FailFinder f;
        sub.getBody().visit(f);
        bool result = f.canFail;
        if (!result && depth < 3) {
            for (const auto* callee : f.callees) {
                if (routineCanFail(*callee, depth + 1)) { result = true; break; }
            }
        }
        memo[&sub] = result;
        return result;
    }
};

} // namespace

class CheckerCannotFail : public Rule {
public:
    std::string_view getId() const override { return "UVM-TST-001"; }
    std::string_view getName() const override { return "CheckerCannotFail"; }
    std::string_view getDescription() const override {
        return "Flags scoreboard and checker classes with no way to report a failure: no `uvm_error or "
               "`uvm_fatal, no assertion and no comparator, in the class, its bases or the helpers it "
               "calls (Sonar S2699, tests without assertions).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        Analyzer analyzer;
        auto classes = swe::userClasses(model);
        // Base classes are judged through the classes that extend them.
        std::unordered_set<const Symbol*> extended;
        for (const auto* cls : classes) {
            if (const Type* base = cls->getBaseClass()) {
                if (const auto* bc = swe::asClass(*base)) {
                    extended.insert(bc);
                    if (bc->genericClass) extended.insert(bc->genericClass);
                }
            }
        }
        for (const auto* cls : classes) {
            if (cls->isAbstract || cls->isInterface || !looksLikeChecker(*cls)) continue;
            if (extended.count(cls) || (cls->genericClass && extended.count(cls->genericClass))) continue;
            if (analyzer.classCanFail(*cls)) continue;
            diags.addViolation(
                "UVM-TST-001", "CheckerCannotFail",
                "Checker '" + std::string(cls->name) +
                    "' never reports an error: it has no `uvm_error/`uvm_fatal, assertion or comparator. "
                    "Every test using it passes regardless of what the DUT does.",
                core::Severity::Error, cls->location);
        }
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCheckerCannotFail() {
    return std::make_unique<errors::CheckerCannotFail>();
}
}
