#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/AssignmentExpressions.h"

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

class SelfAssignmentVisitor : public swe::UserCodeVisitor<SelfAssignmentVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit SelfAssignmentVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const AssignmentExpression& assign) {
        if (!assign.isCompound() && !assign.isNonBlocking() && !assign.timingControl &&
            !assign.isLValueArg()) {
            const Expression& lhs = assign.left();
            const Expression& rhs = swe::stripConversions(assign.right());
            if ((lhs.kind == ExpressionKind::NamedValue || lhs.kind == ExpressionKind::MemberAccess) &&
                lhs.isEquivalentTo(rhs)) {
                std::string name = swe::exprText(lhs);
                std::string hint;
                if (const auto* sym = swe::referencedSymbol(lhs);
                    sym && sym->kind == SymbolKind::FormalArgument) {
                    hint = " The argument '" + std::string(sym->name) +
                           "' hides the class property of the same name; write 'this." +
                           std::string(sym->name) + " = " + std::string(sym->name) + ";'.";
                }
                diags.addViolation("SV-BUG-003", "SelfAssignment",
                                   "'" + name + "' is assigned to itself, so the statement has no effect." +
                                       hint,
                                   core::Severity::Warning, swe::locOf(assign));
            }
        }
        visitDefault(assign);
    }

private:
    core::DiagnosticEngine& diags;
};

class SelfAssignment : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-003"; }
    std::string_view getName() const override { return "SelfAssignment"; }
    std::string_view getDescription() const override {
        return "Flags 'x = x' assignments, typically a constructor or setter whose argument shadows "
               "the property it was meant to set (clang-tidy bugprone-self-assignment, Sonar S1656).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        SelfAssignmentVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSelfAssignment() {
    return std::make_unique<correctness::SelfAssignment>();
}
}
