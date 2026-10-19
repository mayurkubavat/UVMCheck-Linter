#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::random {

class GlobalRngVisitor : public slang::ast::ASTVisitor<GlobalRngVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit GlobalRngVisitor(core::DiagnosticEngine& diags, std::string_view className)
        : diags(diags), className(className) {}

    void handle(const slang::ast::CallExpression& call) {
        std::string_view name = call.getSubroutineName();
        if (name == "$urandom" || name == "$urandom_range" || name == "$random") {
            diags.addViolation(
                "SV-RND-002",
                "GlobalRngInClass",
                "Invoking global pseudo-random system function '" + std::string(name) +
                "' inside class '" + std::string(className) +
                "'. Global RNG calls consume the process-level PRNG state instead of the object's hierarchical random stream, "
                "destroying simulation seed repeatability and testbench reproducibility. Use 'std::randomize()' or class-scoped RNG.",
                core::Severity::Warning,
                call.sourceRange.start()
            );
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view className;
};

class GlobalRngInClass : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-002"; }
    std::string_view getName() const override { return "GlobalRngInClass"; }
    std::string_view getDescription() const override {
        return "Calling $urandom or $random inside class components or sequences corrupts global thread random "
               "stability. Use std::randomize() or object-scoped randomization to preserve simulation repeatability.";
    }
    std::string_view getCategory() const override { return "Reproducibility"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (!sub.flags.has(slang::ast::MethodFlags::BuiltIn)) {
                        GlobalRngVisitor visitor(diags, cls->name);
                        sub.getBody().visit(visitor);
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createGlobalRngInClass() {
    return std::make_unique<random::GlobalRngInClass>();
}
}
