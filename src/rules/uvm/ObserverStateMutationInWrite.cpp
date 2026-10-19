#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::uvm {

static bool isRootedInSymbol(const slang::ast::Expression& expr, const slang::ast::Symbol& targetSym) {
    const auto* curr = &expr;
    while (curr) {
        if (curr->kind == slang::ast::ExpressionKind::NamedValue) {
            return &curr->as<slang::ast::NamedValueExpression>().symbol == &targetSym;
        } else if (curr->kind == slang::ast::ExpressionKind::MemberAccess) {
            curr = &curr->as<slang::ast::MemberAccessExpression>().value();
        } else if (curr->kind == slang::ast::ExpressionKind::ElementSelect) {
            curr = &curr->as<slang::ast::ElementSelectExpression>().value();
        } else if (curr->kind == slang::ast::ExpressionKind::RangeSelect) {
            curr = &curr->as<slang::ast::RangeSelectExpression>().value();
        } else {
            break;
        }
    }
    return false;
}

class WriteBodyVisitor : public slang::ast::ASTVisitor<WriteBodyVisitor, slang::ast::VisitFlags::AllGood> {
public:
    WriteBodyVisitor(const slang::ast::FormalArgumentSymbol& arg,
                     std::string_view className,
                     core::DiagnosticEngine& diags)
        : arg(arg), className(className), diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        if (isRootedInSymbol(assign.left(), arg)) {
            diags.addViolation(
                "UVM-OBS-001",
                "ObserverStateMutationInWrite",
                "In-place mutation of observed transaction argument '" + std::string(arg.name) +
                "' in 'write()' method of class '" + std::string(className) + "'. "
                "In the UVM Observer pattern, the same transaction handle is broadcast to multiple subscribers "
                "(scoreboards, functional coverage, checkers). Mutating transaction fields directly corrupts data "
                "for all downstream observers. Clone the transaction ($cast(cloned_t, " + std::string(arg.name) + ".clone())) before mutating.",
                core::Severity::Error,
                assign.sourceRange.start()
            );
        }
        visitDefault(assign);
    }

private:
    const slang::ast::FormalArgumentSymbol& arg;
    std::string_view className;
    core::DiagnosticEngine& diags;
};

class ObserverStateMutationInWrite : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBS-001"; }
    std::string_view getName() const override { return "ObserverStateMutationInWrite"; }
    std::string_view getDescription() const override {
        return "Mutating the observed transaction object inside a subscriber's write() method corrupts state "
               "for other concurrent subscribers in the Observer pattern. Clone the transaction before mutating.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "write" && sub.subroutineKind == slang::ast::SubroutineKind::Function) {
                        auto args = sub.getArguments();
                        if (!args.empty() && args[0]->getType().isClass()) {
                            WriteBodyVisitor visitor(*args[0], cls->name, diags);
                            sub.getBody().visit(visitor);
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createObserverStateMutationInWrite() {
    return std::make_unique<uvm::ObserverStateMutationInWrite>();
}
}
