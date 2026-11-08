#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/types/Type.h"

namespace uvmcheck::rules::uvm {

// Visits bad nodes too: against the real UVM library a mismatched connect() does not
// type-check, so slang wraps the call and its statement in Invalid nodes.
/// The type a class specialization's first parameter resolves to, when
/// that parameter is a type (`uvm_analysis_port#(T)` -> what T stands for).
static const slang::ast::Type* firstTypeArg(const slang::ast::Type& type) {
    using namespace slang::ast;
    const auto& t = type.getCanonicalType();
    if (!t.isClass()) return nullptr;
    const auto& cls = t.as<ClassType>();
    if (!cls.genericClass || cls.genericParameters.empty()) return nullptr;
    const auto* p = cls.genericParameters[0];
    if (p->kind != SymbolKind::TypeParameter) return nullptr;
    return &p->as<TypeParameterSymbol>().targetType.getType().getCanonicalType();
}

class ConnectCallVisitor : public slang::ast::ASTVisitor<ConnectCallVisitor,
                                                         slang::ast::VisitFlags::AllGood | slang::ast::VisitFlags::Bad> {
public:
    core::DiagnosticEngine& diags;
    explicit ConnectCallVisitor(core::DiagnosticEngine& d) : diags(d) {}

    void handle(const slang::ast::InvalidStatement& stmt) {
        if (stmt.child) stmt.child->visit(*this);
    }

    void handle(const slang::ast::InvalidExpression& expr) {
        if (expr.child) expr.child->visit(*this);
    }

    void handle(const slang::ast::CallExpression& call) {
        if (call.getSubroutineName() == "connect" && call.thisClass() && !call.arguments().empty()) {
            const auto* arg0 = call.arguments()[0];
            if (arg0) {
                // Unwrap any implicit conversions (e.g. upcasting export to uvm_object) and the
                // InvalidExpression slang puts around an argument that fails to type-check.
                const auto* actualArg = arg0;
                while (actualArg) {
                    if (actualArg->kind == slang::ast::ExpressionKind::Conversion)
                        actualArg = &actualArg->as<slang::ast::ConversionExpression>().operand();
                    else if (actualArg->kind == slang::ast::ExpressionKind::Invalid)
                        actualArg = actualArg->as<slang::ast::InvalidExpression>().child;
                    else
                        break;
                }
                if (!actualArg) {
                    visitDefault(call);
                    return;
                }

                std::string thisType(call.thisClass()->type->toString());
                std::string argType(actualArg->type->toString());

                // Compare the item types themselves: slang prints a
                // specialization with the parameter names of the context
                // that made it first (`T`, `REQ`), so the same item type can
                // print two ways.
                const auto* item1 = firstTypeArg(*call.thisClass()->type);
                const auto* item2 = firstTypeArg(*actualArg->type);
                std::string req1 = item1 ? item1->toString() : "";
                std::string req2 = item2 ? item2->toString() : "";

                if (item1 && item2 && !item1->isMatching(*item2)) {
                    diags.addViolation(
                        "UVM-SEQ-013",
                        "SequenceItemTypeCompatibility",
                        "Transaction item type mismatch in connect(): driver port type '" +
                        thisType + "' (REQ=" + req1 + ") is connected to sequencer export type '" +
                        argType + "' (REQ=" + req2 + "). "
                        "Mismatched item parameters will cause runtime slicing or dynamic cast failures.",
                        core::Severity::Warning,
                        call.sourceRange.start()
                    );
                }
            }
        }
        visitDefault(call);
    }
};

class SequenceItemTypeCompatibility : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-013"; }
    std::string_view getName() const override { return "SequenceItemTypeCompatibility"; }
    std::string_view getDescription() const override {
        return "Statically verifies that transaction item types between uvm_driver#(REQ) and "
               "uvm_sequencer#(REQ) are type-compatible across connect_phase connections.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        ConnectCallVisitor visitor(diags);
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "connect_phase") {
                        sub.getBody().visit(visitor);
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceItemTypeCompatibility() {
    return std::make_unique<uvm::SequenceItemTypeCompatibility>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSequenceItemTypeCompatibility)
