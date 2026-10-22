#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class InvocationCounter : public slang::syntax::SyntaxVisitor<InvocationCounter> {
public:
    size_t raiseCount = 0;
    size_t dropCount = 0;
    slang::SourceLocation firstRaiseLoc;

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        std::string methodName = std::string(inv.left->getLastToken().valueText());
        if (methodName == "raise_objection") {
            raiseCount++;
            if (!firstRaiseLoc.valid()) {
                firstRaiseLoc = inv.getFirstToken().location();
            }
        } else if (methodName == "drop_objection") {
            dropCount++;
        }
        visitDefault(inv);
    }
};

class ObjectionMismatchVisitor : public engine::SyntaxVisitor<ObjectionMismatchVisitor> {
public:
    explicit ObjectionMismatchVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassMethodDeclarationSyntax& method) {
        if (method.declaration) {
            handle(*method.declaration);
        }
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        InvocationCounter counter;
        for (const auto* item : func.items) {
            item->visit(counter);
        }

        if (counter.raiseCount > counter.dropCount && counter.firstRaiseLoc.valid()) {
            std::string routineName = std::string(func.prototype->name->getLastToken().valueText());
            diags.addViolation(
                "UVM-PHS-006",
                "PhaseObjectionMismatch",
                "UVM phase objection raised via 'raise_objection()' in '" + routineName +
                "()' without matching 'drop_objection()'. Unpaired phase objections prevent phase completion and cause simulation to hang indefinitely.",
                core::Severity::Error,
                counter.firstRaiseLoc
            );
        }

        visitDefault(func);
    }

private:
    core::DiagnosticEngine& diags;
};

class PhaseObjectionMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-006"; }
    std::string_view getName() const override { return "PhaseObjectionMismatch"; }
    std::string_view getDescription() const override {
        return "Detects 'phase.raise_objection()' calls without a matching 'phase.drop_objection()' in the same task or method. "
               "Unpaired objections prevent UVM phases from terminating, causing simulation hangs.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ObjectionMismatchVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPhaseObjectionMismatch() {
    return std::make_unique<uvm::PhaseObjectionMismatch>();
}
}
