#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include <string>

namespace uvmcheck::rules::random {

class RandomizeInConstructorVisitor : public slang::ast::ASTVisitor<RandomizeInConstructorVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit RandomizeInConstructorVisitor(core::DiagnosticEngine& diags, std::string_view className)
        : diags(diags), className(className) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.getSubroutineName() == "randomize") {
            diags.addViolation(
                "SV-RND-018",
                "RandomizeInConstructor",
                "Calling 'randomize()' inside class constructor 'new()' of class '" + std::string(className) +
                "'. In SystemVerilog OOP, calling randomize() during object construction executes before derived class members, "
                "constraints, and factory overrides are fully constructed and initialized. "
                "Stimulus randomization should always occur explicitly on fully constructed instances (e.g. 'if (!item.randomize()) ...').",
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

class RandomizeInConstructor : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-018"; }
    std::string_view getName() const override { return "RandomizeInConstructor"; }
    std::string_view getDescription() const override {
        return "Disallows calling randomize() inside class constructors (new()). "
               "Randomizing during construction breaks derived-class constraint inheritance and factory overrides.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "new" && !sub.flags.has(slang::ast::MethodFlags::BuiltIn)) {
                        RandomizeInConstructorVisitor visitor(diags, cls->name);
                        sub.getBody().visit(visitor);
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomizeInConstructor() {
    return std::make_unique<random::RandomizeInConstructor>();
}
}
