#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/LiteralExpressions.h"

namespace uvmcheck::rules::readability {

using namespace slang::ast;

namespace {

bool isFlagLiteral(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    if (x.kind != ExpressionKind::IntegerLiteral) return false;
    auto v = x.as<IntegerLiteral>().getValue().as<int64_t>();
    return v && (*v == 0 || *v == 1);
}

class FlagVisitor : public swe::UserCodeVisitor<FlagVisitor> {
public:
    using UserCodeVisitor::handle;

    FlagVisitor(core::DiagnosticEngine& diags, int64_t minFlags) : diags(diags), minFlags(minFlags) {}

    void handle(const CallExpression& call) {
        check(call);
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
    int64_t minFlags;

    void check(const CallExpression& call) {
        if (call.isSystemCall()) return;
        const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
        if (!sub || !*sub || swe::isLibrarySymbol(**sub)) return;
        int n = swe::positionalArgCount(call);
        if (n <= 0) return;
        auto formals = (*sub)->getArguments();
        auto actuals = call.arguments();
        std::vector<std::string> names;
        for (size_t i = 0; i < static_cast<size_t>(n) && i < formals.size() && i < actuals.size(); i++) {
            if (formals[i]->getType().getBitWidth() == 1 && isFlagLiteral(*actuals[i]))
                names.push_back(std::string(formals[i]->name));
        }
        if (static_cast<int64_t>(names.size()) < minFlags) return;
        std::string named;
        for (const auto& nm : names) {
            if (!named.empty()) named += ", ";
            named += "." + nm + "(...)";
        }
        diags.addViolation(
            "SV-RDB-001", "PositionalFlagLiterals",
            "Call to '" + std::string((*sub)->name) + "' passes " + std::to_string(names.size()) +
                " positional 0/1 flags. A reader cannot tell which is which, and adding an argument "
                "shifts them silently; bind them by name: " + named + ".",
            core::Severity::Info, swe::locOf(call));
    }
};

} // namespace

class PositionalFlagLiterals : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-001"; }
    std::string_view getName() const override { return "PositionalFlagLiterals"; }
    std::string_view getDescription() const override {
        return "Flags calls that pass two or more 0/1 literals positionally to 1-bit arguments; use "
               "named argument binding (clang-tidy bugprone-argument-comment, Clean Code flag "
               "arguments).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        FlagVisitor v(diags, swe::ruleOption("SV-RDB-001", "min_flags", 2));
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPositionalFlagLiterals() {
    return std::make_unique<readability::PositionalFlagLiterals>();
}
}
