#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::random {

class SuperRandomizeVisitor : public slang::ast::ASTVisitor<SuperRandomizeVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit SuperRandomizeVisitor(std::string_view expectedMethod) : expectedMethod(expectedMethod) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.subroutine.index() == 0) {
            const auto* sub = std::get<0>(call.subroutine);
            if (sub && sub->name == expectedMethod) {
                if (call.syntax) {
                    std::string text(call.syntax->toString());
                    if (text.find("super.") != std::string::npos) {
                        callsSuper = true;
                    }
                }
            }
        }
        visitDefault(call);
    }

    bool callsSuper = false;
    std::string_view expectedMethod;
};

class RandomizeMissingSuper : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-003"; }
    std::string_view getName() const override { return "RandomizeMissingSuper"; }
    std::string_view getDescription() const override {
        return "Overridden pre_randomize() and post_randomize() callback methods must invoke super.pre_randomize() "
               "or super.post_randomize() to prevent bypassing base-class transaction setup and validation routines.";
    }
    std::string_view getCategory() const override { return "Safety"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !cls->getBaseClass()) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "pre_randomize" || sub.name == "post_randomize") {
                        if (!sub.flags.has(slang::ast::MethodFlags::BuiltIn)) {
                            SuperRandomizeVisitor visitor(sub.name);
                            sub.getBody().visit(visitor);

                            if (!visitor.callsSuper) {
                                diags.addViolation(
                                    "SV-RND-003",
                                    "RandomizeMissingSuper",
                                    "Overridden randomization callback '" + std::string(sub.name) + "' in class '" +
                                    std::string(cls->name) + "' does not invoke 'super." + std::string(sub.name) + "()'. "
                                    "Unlike UVM phases, SV pre/post_randomize callbacks do not auto-chain up inheritance; "
                                    "omitting super bypasses base-class payload, CRC, and sequence updates.",
                                    core::Severity::Warning,
                                    sub.location
                                );
                            }
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomizeMissingSuper() {
    return std::make_unique<random::RandomizeMissingSuper>();
}
}
