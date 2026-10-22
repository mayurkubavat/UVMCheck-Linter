#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/AssignmentExpressions.h"

#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

namespace {

std::string lower(std::string_view s) {
    std::string out(s);
    std::transform(out.begin(), out.end(), out.begin(), [](unsigned char c) { return std::tolower(c); });
    return out;
}

// Name of a plain variable or member argument ("addr" for tr.addr), else "".
std::string argName(const Expression& arg) {
    const Expression* e = &arg;
    if (e->kind == ExpressionKind::Assignment) e = &e->as<AssignmentExpression>().left();
    const Symbol* sym = swe::referencedSymbol(*e);
    return sym ? lower(sym->name) : std::string();
}

} // namespace

class SuspiciousArgumentOrderVisitor : public swe::UserCodeVisitor<SuspiciousArgumentOrderVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit SuspiciousArgumentOrderVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const CallExpression& call) {
        check(call);
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;

    void check(const CallExpression& call) {
        if (call.isSystemCall()) return;
        const auto* subp = std::get_if<const SubroutineSymbol*>(&call.subroutine);
        if (!subp || !*subp || swe::isLibrarySymbol(**subp)) return;
        auto formals = (*subp)->getArguments();
        auto actuals = call.arguments();
        if (formals.size() < 2) return;
        int n = swe::positionalArgCount(call);
        if (n < 2) return;
        size_t count = std::min({static_cast<size_t>(n), formals.size(), actuals.size()});
        for (size_t i = 0; i < count; i++) {
            std::string name = argName(*actuals[i]);
            if (name.empty() || name == lower(formals[i]->name)) continue;
            for (size_t j = 0; j < formals.size(); j++) {
                if (j == i || lower(formals[j]->name) != name) continue;
                diags.addViolation(
                    "SV-BUG-007", "SuspiciousArgumentOrder",
                    "'" + swe::exprText(*actuals[i]) + "' is passed as argument " + std::to_string(i + 1) +
                        " ('" + std::string(formals[i]->name) + "') of '" + std::string((*subp)->name) +
                        "', but the routine has a parameter named '" + std::string(formals[j]->name) +
                        "' at position " + std::to_string(j + 1) +
                        ". The arguments look swapped; check the order or use named arguments.",
                    core::Severity::Warning, swe::locOf(*actuals[i]));
                return;
            }
        }
    }
};

class SuspiciousArgumentOrder : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-007"; }
    std::string_view getName() const override { return "SuspiciousArgumentOrder"; }
    std::string_view getDescription() const override {
        return "Flags positional calls where an argument's name matches a different parameter of the "
               "callee, such as write(len, addr) for write(addr, len) (clang-tidy "
               "readability-suspicious-call-argument, Sonar S2234).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        SuspiciousArgumentOrderVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSuspiciousArgumentOrder() {
    return std::make_unique<correctness::SuspiciousArgumentOrder>();
}
}
